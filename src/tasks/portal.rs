//! Nether portal by obsidian casting (no diamond pickaxe) + portal entry.
//!
//! Port of steve's `tasks/portal/{cast,enter}.ts`. Each obsidian block is cast in
//! place: a fully-enclosed 1-block cup holds a lava source, then water poured into
//! the block directly above flows down and turns it to obsidian. The cup contains
//! the lava so it never reaches the bot. The 4x5 frame (10 obsidian, no corners) is
//! cast bottom-up, the 2x3 interior is dug out, and it's lit with flint & steel.

use std::time::{Duration, Instant};

use crate::bot::{Bot, Face};
use crate::vec3::{vec3, Vec3};

use crate::bot_utils::{count_items, select_item};
use crate::memory::{PoiKind, PoiStatus, WorldMemory};
use crate::tasks::mining::{descend_step, dig_down, ensure_pickaxe, strip_tunnel};
use crate::types::{failure, success, StepResult};

// ── block classification ────────────────────────────────────────────────────

/// Debug log for the cast (to stderr → the bot log) when CRAFT_DEBUG is set.
/// The last cast debug line (always kept, logging or not) — the gym puts it in a timeout FAIL so
/// gym.db says WHERE the cast was stuck instead of a bare "gym timeout".
static LAST_CAST_LINE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
pub fn last_cast_line() -> String {
    LAST_CAST_LINE.lock().unwrap().clone()
}

/// Process start, for the elapsed-seconds stamp on cast lines (the logs had no time at all, so a
/// 33-minute 8-cell mold could not be broken down).
static CAST_T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

pub(crate) fn cast_debug(msg: &str) {
    *LAST_CAST_LINE.lock().unwrap() = msg.chars().take(160).collect();
    if std::env::var("CRAFT_DEBUG").is_ok() {
        let t = CAST_T0.get_or_init(std::time::Instant::now).elapsed().as_secs();
        eprintln!("    CAST [{t:>5}s] {msg}");
    }
}

/// Block name at (x,y,z), or "air" when empty.
pub(crate) fn name_at(bot: &Bot, x: i32, y: i32, z: i32) -> String {
    bot.block_at(x, y, z).map(|b| b.name).unwrap_or_else(|| "air".into())
}

pub(crate) fn is_air(n: &str) -> bool {
    n == "air" || n == "cave_air" || n == "void_air"
}

const SOFT: &[&str] = &["short_grass", "tall_grass", "fern", "snow", "snow_layer", "dead_bush"];

fn is_replaceable(n: &str) -> bool {
    is_air(n) || SOFT.contains(&n)
}

pub(crate) fn is_solid(n: &str) -> bool {
    !is_replaceable(n) && !n.contains("water") && !n.contains("lava")
}

pub(crate) fn is_lava(n: &str) -> bool {
    n == "lava" || n == "flowing_lava"
}

/// Is (x,y,z) a SOURCE fluid block (level=0)? Only source blocks can be picked up with a
/// bucket — flowing fluid (level 1-8) scoops nothing. The lava-search finds cave-edge lava
/// that's frequently FLOWING, so the scoop failed ("all rounds failed") even at 1 block away;
/// targeting source blocks fixes it. Missing/empty level is treated as source (still a lake).
pub(crate) fn is_fluid_source(bot: &Bot, x: i32, y: i32, z: i32, fluid: &str) -> bool {
    bot.block_at(x, y, z)
        .map(|b| {
            (b.name == fluid || b.name.contains(fluid))
                && b.properties.get("level").map(|l| l == "0").unwrap_or(true)
        })
        .unwrap_or(false)
}

pub(crate) fn solid_at(bot: &Bot, x: i32, y: i32, z: i32) -> bool {
    is_solid(&name_at(bot, x, y, z))
}

/// First SOLID cell the segment `from → to` passes through (sampled every 0.1), excluding the
/// `target` cell itself. None = clear line of sight to the target.
pub(crate) fn first_solid_on_ray(bot: &Bot, from: Vec3, to: Vec3, target: (i32, i32, i32)) -> Option<(i32, i32, i32)> {
    let d = to.subtract(from);
    let len = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt();
    if len < 1e-6 {
        return None;
    }
    let steps = (len / 0.1).ceil() as i32;
    let mut last: Option<(i32, i32, i32)> = None;
    for i in 1..=steps {
        let f = i as f64 / steps as f64;
        let p = vec3(from.x + d.x * f, from.y + d.y * f, from.z + d.z * f);
        let c = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if Some(c) == last {
            continue;
        }
        last = Some(c);
        if c == target {
            return None;
        }
        if solid_at(bot, c.0, c.1, c.2) {
            return Some(c);
        }
    }
    None
}

/// Is there ANY lava block (exposed or sealed) within `r` of the bot? Cheap raw scan
/// used to STOP descending before the bot blunders into a lava lake — dig_down and
/// descend_step avoid lava, but the relocate goto_xz will happily path a bot straight
/// through one at depth (with blocks_cant_break cleared). Near lava, scoop; don't walk.
pub(crate) fn raw_lava_near(bot: &Bot, r: i32) -> bool {
    let p = bot.entity.position;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    for dx in -r..=r {
        for dy in -r..=r {
            for dz in -r..=r {
                if is_lava(&name_at(bot, bx + dx, by + dy, bz + dz)) {
                    return true;
                }
            }
        }
    }
    false
}

/// Is there a SCOOPABLE source lava block within `r` of the bot — a level=0 block with AIR
/// directly above it? The descent settles to scoop only when this is true. The air-above
/// test is what fill_bucket also requires: a lava body's submerged interior is level=0 too,
/// but capped by lava (not scoopable), so matching only exposed sources here keeps the
/// descent from settling on a flowing pocket / lava lake it can't actually fill from
/// (seen live: "0 sources, 131 lava blocks" at y=-12 where interior sources exist).
fn source_lava_near(bot: &Bot, r: i32) -> bool {
    let p = bot.entity.position;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    for dx in -r..=r {
        for dy in -r..=r {
            for dz in -r..=r {
                let (x, y, z) = (bx + dx, by + dy, bz + dz);
                if is_fluid_source(bot, x, y, z, "lava") && is_air(&name_at(bot, x, y + 1, z)) {
                    return true;
                }
            }
        }
    }
    false
}

pub(crate) fn feet_y(bot: &Bot) -> i32 {
    bot.entity.position.y.floor() as i32
}

/// The cheap throwaway block we scaffold/mould with. cobbled_deepslate counts: the
/// cast happens at lava depth (y≈-50) where mined stone is DEEPSLATE → cobbled_deepslate,
/// not cobblestone. Ignoring it stranded portal-ready bots looping "need ~30 cobble"
/// with a full stack of deepslate cobble the step gate (which counts both) let through.
pub(crate) fn build_block(bot: &Bot) -> &'static str {
    if count_items(bot, "cobblestone") > 0 {
        "cobblestone"
    } else if count_items(bot, "cobbled_deepslate") > 0 {
        "cobbled_deepslate"
    } else {
        "dirt"
    }
}

/// Total throwaway scaffold blocks on hand (cobble of either kind + dirt).
pub(crate) fn scaffold_count(bot: &Bot) -> i32 {
    count_items(bot, "cobblestone") + count_items(bot, "cobbled_deepslate") + count_items(bot, "dirt")
}

/// The face on the reference block `ref = pos + d` that points back toward `pos`
/// (i.e. the face whose offset is `-d`), so `place_block(ref, face)` lands at `pos`.
fn face_back(d: (i32, i32, i32)) -> Face {
    match (-d.0, -d.1, -d.2) {
        (0, -1, 0) => Face::Bottom,
        (0, 1, 0) => Face::Top,
        (0, 0, -1) => Face::North,
        (0, 0, 1) => Face::South,
        (-1, 0, 0) => Face::West,
        (1, 0, 0) => Face::East,
        _ => Face::Top,
    }
}

// ── primitive actions ───────────────────────────────────────────────────────

/// Use the held item (bucket / flint&steel) while looking at `look`.
pub(crate) async fn reliable_use(bot: &mut Bot<'_>, look: Vec3) {
    bot.look_at(look);
    bot.wait_ticks(7).await.ok();
    bot.activate_item().await.ok();
    bot.wait_ticks(15).await.ok();
}

/// Dig the block at (x,y,z) unless it's air or obsidian (an iron pick can't break
/// obsidian and the dig would hang).
pub(crate) async fn dig_at(bot: &mut Bot<'_>, x: i32, y: i32, z: i32) {
    let n = name_at(bot, x, y, z);
    if is_air(&n) || n == "obsidian" || n.contains("lava") {
        return;
    }
    // Dig with a PICKAXE, not whatever bucket is in hand: a bare-hand cobble dig takes 10s and
    // `bot.dig` only reflects sub-4s breaks locally, so the water-lid clear at (910,-52,464)
    // "failed" (local view stale) and cost a whole attempt cycle before the next pass saw air.
    let _ = ensure_pickaxe(bot).await;
    bot.look_at(vec3(x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5));
    bot.wait_ticks(2).await.ok();
    let _ = bot.dig(x, y, z).await;
    bot.wait_ticks(3).await.ok();
}

/// Place a build block at `pos` against any solid neighbour. True once solid.
pub(crate) async fn place_cobble(bot: &mut Bot<'_>, pos: (i32, i32, i32)) -> bool {
    if solid_at(bot, pos.0, pos.1, pos.2) {
        return true;
    }
    if !select_item(bot, build_block(bot)).await.unwrap_or(false) {
        return false;
    }
    // Prefer a downward/side neighbour to click; top last.
    for d in [(0, -1, 0), (1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0)] {
        let r = (pos.0 + d.0, pos.1 + d.1, pos.2 + d.2);
        if !solid_at(bot, r.0, r.1, r.2) {
            continue;
        }
        bot.look_at(vec3(pos.0 as f64 + 0.5, pos.1 as f64 + 0.5, pos.2 as f64 + 0.5));
        bot.wait_ticks(2).await.ok();
        let _ = bot.place_block(r.0, r.1, r.2, face_back(d)).await;
        // Cycle 5 (6b-safe): judge the cap by the server's answer. `place_block` predicts the block
        // client-side at once, and a rejected placement is reverted by a block_update a few ticks
        // later, so a 3-tick re-read counted rejected caps as placed (6c+lsm13: a "capped" lava cell
        // the bot then stepped into). Never by an inventory count (the SDK doesn't decrement it).
        bot.wait_ticks(8).await.ok();
        if solid_at(bot, pos.0, pos.1, pos.2) {
            return true;
        }
    }
    false
}

/// Eat cooked food when hurt and safe. The portal build takes lava nicks the bot can't otherwise
/// recover from — it has no other eat path, and natural regen only fires at foodLevel>=18, which a
/// long build drains below. Eating keeps food topped up so regen stays active, turning a nick into
/// a survivable event instead of a slow slide to death. Only eats when health is low AND no lava is
/// adjacent (never mid-hazard). activate_item sets the server-side active item; the server ticks
/// the use to completion, so a plain activate + wait finishes the meal.
pub(crate) async fn eat_if_hurt(bot: &mut Bot<'_>) {
    if bot.health >= 17.0 {
        return;
    }
    // Normally don't stop to eat next to lava (mid-hazard). BUT the whole cast happens
    // within 3 of lava, so the old blanket `raw_lava_near` guard meant the bot NEVER ate
    // during the cast → hunger drained → natural regen (foodLevel>=18) switched off → a
    // lava nick that regen would have shrugged off instead slid to 0 (the reproduce stalled
    // at 9/10 stuck "health 0->0"). So near lava, still eat when CRITICALLY low to keep
    // regen alive; only skip the near-lava eat at moderate health.
    if raw_lava_near(bot, 3) && bot.health >= 10.0 {
        return;
    }
    // Prefer cooked, but a RACE bot doesn't cook mid-build — accept raw meat too (raw still
    // tops up hunger enough for natural regen). Ordered cooked-first so it eats the better food.
    const FOODS: [&str; 12] = [
        "cooked_beef", "cooked_porkchop", "cooked_mutton", "cooked_chicken", "cooked_rabbit", "bread",
        "beef", "porkchop", "mutton", "chicken", "rabbit", "cooked_cod",
    ];
    let Some(food) = FOODS.iter().find(|f| count_items(bot, f) > 0) else {
        return;
    };
    if !select_item(bot, food).await.unwrap_or(false) {
        return;
    }
    let before = bot.health;
    for _ in 0..4 {
        if bot.health >= 18.0 {
            break;
        }
        // Look flat/away so activate doesn't interact with a block face; then hold the use.
        let p = bot.entity.position;
        bot.look_at(vec3(p.x, p.y + 1.0, p.z + 3.0));
        bot.wait_ticks(2).await.ok();
        bot.activate_item().await.ok();
        bot.wait_ticks(40).await.ok();
    }
    cast_debug(&format!("eat: health {before:.0}->{:.0}", bot.health));
}

/// Place a block at `pos`, building a foundation straight down when it floats in
/// air with no neighbour to place against.
pub(crate) async fn ensure_solid(bot: &mut Bot<'_>, pos: (i32, i32, i32), depth: i32) -> bool {
    if solid_at(bot, pos.0, pos.1, pos.2) {
        return true;
    }
    if place_cobble(bot, pos).await {
        return true;
    }
    if depth >= 4 {
        return false;
    }
    if Box::pin(ensure_solid(bot, (pos.0, pos.1 - 1, pos.2), depth + 1)).await {
        return place_cobble(bot, pos).await;
    }
    false
}

/// Walk to within `target_dist` of (tx,tz) under manual control (precise centering
/// the pathfinder won't give). Jumps when stuck or in water.
pub(crate) async fn walk_to_xz(bot: &mut Bot<'_>, tx: f64, tz: f64, target_dist: f64, max_ticks: u32) {
    // A raw walk only FINISHES a short last stretch (≤ 3.5 blocks); longer moves belong to the
    // lava-aware pathfinder. While `max_ticks` was a packet count every call moved ~0.2–0.6 blocks;
    // with real ticks a caller that ran it after a goto stopped far short walked straight across the
    // gap toward a lava-side stand (batch 10, and the batch A restart: −51 → −55.6 in 5 s).
    {
        let p = bot.entity.position;
        if ((tx - p.x).powi(2) + (tz - p.z).powi(2)).sqrt() > 3.5 {
            return;
        }
    }
    let mut prev = f64::MAX;
    for _ in 0..max_ticks {
        let p = bot.entity.position;
        let d = ((tx - p.x).powi(2) + (tz - p.z).powi(2)).sqrt();
        if d < target_dist {
            break;
        }
        let (px, py, pz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        // LAVA-SAFETY: this is a RAW control-state walk (no pathfinding), so unlike goto*
        // it will happily stride into lava. Re-scooping from the deep lava sea, that walked
        // portal bots straight into the ocean and killed them (died at y=-58, surface -55,
        // before casting a single obsidian). Stop before stepping onto a cell that IS lava
        // or whose floor is lava (a fall-in). The scoop stand-spot is solid ground ADJACENT
        // to the lava, so we still reach it — we just never overshoot into the sea.
        let sx = if tx > p.x + 0.3 { 1 } else if tx < p.x - 0.3 { -1 } else { 0 };
        let sz = if tz > p.z + 0.3 { 1 } else if tz < p.z - 0.3 { -1 } else { 0 };
        // Also a FLOORLESS cell with lava around it: a scooped source leaves an air hole in the
        // rim at pool level; stepping into it drops the bot into the lake (rust-gym-001: feet −54 →
        // −55 at hp 0 between `fill lava: ENTER` and the first scoop, lava surface −55).
        let lava_cell = |bot: &Bot, dx: i32, dz: i32| {
            let (x, z) = (px + dx, pz + dz);
            let floorless = !is_solid(&name_at(bot, x, py - 1, z));
            name_at(bot, x, py, z).contains("lava")
                || name_at(bot, x, py - 1, z).contains("lava")
                || (floorless
                    && (-1..=1).any(|ox| (-1..=1).any(|oz| (1..=2).any(|oy| is_lava(&name_at(bot, x + ox, py - oy, z + oz))))))
        };
        let lava_ahead = (sx != 0 && lava_cell(bot, sx, 0))
            || (sz != 0 && lava_cell(bot, 0, sz))
            || (sx != 0 && sz != 0 && lava_cell(bot, sx, sz));
        if lava_ahead {
            break; // don't walk into / over the lava sea
        }
        let in_water = name_at(bot, px, py, pz).contains("water")
            || name_at(bot, px, py + 1, pz).contains("water");
        let stuck = d > prev - 0.05;
        prev = d;
        bot.look_at(vec3(tx, p.y, tz));
        bot.set_control_state("forward", true);
        if stuck || in_water {
            bot.set_control_state("jump", true);
        }
        // A real game tick. `drive_tick` returns per network PACKET, so `max_ticks` = 30 used to
        // last milliseconds: tunnel steps "didn't move" (moved 0.2–0.6 per call, batch 4/5 detour
        // logs) and the at-depth tunnel sat at hd 12 for 14 min.
        if bot.wait_ticks(1).await.is_err() {
            break;
        }
        bot.set_control_state("jump", false);
    }
    bot.set_control_state("forward", false);
    bot.set_control_state("jump", false);
}

/// Dig straight down (every cell under the footprint) until feet reach `target_y`.
pub(crate) async fn descend_to_y(bot: &mut Bot<'_>, target_y: i32) {
    for _ in 0..24 {
        if feet_y(bot) <= target_y {
            break;
        }
        let f_before = feet_y(bot);
        let p = bot.entity.position;
        let f = feet_y(bot);
        let mut cells = Vec::new();
        for dx in [-0.3, 0.3] {
            for dz in [-0.3, 0.3] {
                let c = ((p.x + dx).floor() as i32, (p.z + dz).floor() as i32);
                if !cells.contains(&c) {
                    cells.push(c);
                }
            }
        }
        let mut dug = 0;
        for (cx, cz) in cells {
            let n = name_at(bot, cx, f - 1, cz);
            if !is_solid(&n) || n == "obsidian" {
                continue;
            }
            // Lava-safety: never dig a floor block that drops us onto/into lava, nor one
            // with lava beside it (it floods the hole). If lava is this close we've
            // reached the lava layer — leave it be so the caller's find_fluid can scoop
            // the now-nearby exposed source instead of the bot drowning in it.
            let lava_below = is_lava(&name_at(bot, cx, f - 2, cz));
            let lava_beside = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|&(dx, dz)| is_lava(&name_at(bot, cx + dx, f - 1, cz + dz)));
            if lava_below || lava_beside {
                cast_debug(&format!("DESC y={f} ({cx},{cz}) LAVA-STOP below2={}", name_at(bot, cx, f - 2, cz)));
                continue;
            }
            // Only onto solid ground: an air pocket under the dug block dropped rust-gym-002 from
            // -52 to -56 (below the lava surface -55) during a refill descent — dead 3 s later.
            if !is_solid(&name_at(bot, cx, f - 2, cz)) {
                cast_debug(&format!("DESC y={f} ({cx},{cz}) AIR-STOP below2={}", name_at(bot, cx, f - 2, cz)));
                continue;
            }
            dig_at(bot, cx, f - 1, cz).await;
            dug += 1;
        }
        cast_debug(&format!("DESC y={f} dug={dug}"));
        bot.wait_ticks(8).await.ok();
        // Bail the instant an iteration fails to LOWER the bot: over a lava pool the footprint's
        // lava cells trip LAVA-STOP while a lone non-lava cell digs forever without ever dropping
        // us (dug=1 but feet unchanged). That spun descend_to_y 24× per call and, re-called by the
        // cast's attempt/frame loops, logged thousands of LAVA-STOP and burned the whole budget.
        // If we can't get lower here, stop — the caller casts from this (base_y+1) level instead.
        if feet_y(bot) >= f_before {
            break;
        }
    }
}

/// Punch straight DOWN through a water aquifer (dig_down/descend_step both refuse all
/// liquid, so a water pocket otherwise stalls the lava-search into endless relocation and
/// the bot never reaches the deep lava). This digs the solid floor beneath the water column
/// and lets the bot sink through — LAVA is still refused (fatal), only water is punched. Runs
/// inside the single build_nether_portal call, so survival can't surface the bot mid-punch.
/// Bounded (≤6 blocks) + returns whether it actually descended, so the caller falls back to
/// relocation when there's nothing to punch.
pub(crate) async fn punch_through_water(bot: &mut Bot<'_>) -> bool {
    // Declared water work: the breath watchdog must not hold jump against a deliberate sink.
    // It is bounded (≤6 blocks); the alarm still fires for the step after it returns.
    bot.allow_underwater = true;
    let r = punch_through_water_inner(bot).await;
    bot.allow_underwater = false;
    r
}

async fn punch_through_water_inner(bot: &mut Bot<'_>) -> bool {
    let start_y = bot.entity.position.y;
    let x = bot.entity.position.x.floor() as i32;
    let z = bot.entity.position.z.floor() as i32;
    let fy = (bot.entity.position.y - 0.5).floor() as i32;
    // Only engage when WATER is genuinely what's blocking the descent (that's why dig_down
    // refused). If it's dry, leave it to the normal descent/relocate path.
    let water_here = name_at(bot, x, fy, z).contains("water")
        || name_at(bot, x, fy - 1, z).contains("water")
        || [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .iter()
            .any(|&(dx, dz)| name_at(bot, x + dx, fy, z + dz).contains("water"));
    if !water_here {
        return false;
    }
    for _ in 0..6 {
        let feet = (bot.entity.position.y - 0.5).floor() as i32;
        if feet <= bot.game.min_y + 4 {
            break;
        }
        let target = feet - 1;
        // Never dig toward lava — below, two-below, or beside the target floor block.
        let lava_adj = is_lava(&name_at(bot, x, target, z))
            || is_lava(&name_at(bot, x, target - 1, z))
            || [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|&(dx, dz)| is_lava(&name_at(bot, x + dx, target, z + dz)));
        if lava_adj {
            break;
        }
        let below = name_at(bot, x, target, z);
        if is_solid(&below) && below != "obsidian" {
            dig_at(bot, x, target, z).await;
        }
        // Sink into the opened space. Water gravity is weak, so look down and give it a few
        // ticks to settle before digging the next floor block.
        let p = bot.entity.position;
        bot.look_at(vec3(p.x, p.y - 3.0, p.z));
        bot.wait_ticks(10).await.ok();
    }
    bot.entity.position.y < start_y - 0.9
}

/// Drop through a thin solid floor into an open CAVERN below — caves are where exposed lava
/// lakes live, but dig_down refuses to plunge (fall-avoidance) so the descent stalls on the
/// cave roof. Digs at most a 3-block floor cap, then only drops when the landing is within a
/// survivable fall and there's NO lava in the shaft or at the landing. Returns whether it descended.
pub(crate) async fn drop_into_cavern(bot: &mut Bot<'_>) -> bool {
    let start_y = bot.entity.position.y;
    let x = bot.entity.position.x.floor() as i32;
    let z = bot.entity.position.z.floor() as i32;
    let feet = (bot.entity.position.y - 0.5).floor() as i32;
    // Dig through at most a 3-block floor cap to reach a cave opening.
    let mut cap = 0;
    let mut y = feet - 1;
    while cap < 3 && is_solid(&name_at(bot, x, y, z)) {
        if is_lava(&name_at(bot, x, y, z)) {
            return false;
        }
        cap += 1;
        y -= 1;
    }
    if is_solid(&name_at(bot, x, y, z)) {
        return false; // no cavern within 3 blocks — not our case, leave to relocate
    }
    // Fall through the air column to the landing; bail on lava or an unsurvivable depth.
    while y > bot.game.min_y + 2 && !is_solid(&name_at(bot, x, y, z)) {
        if is_lava(&name_at(bot, x, y, z)) {
            return false;
        }
        y -= 1;
    }
    let landing = y + 1; // top face of the cave floor we'd land on
    if feet - landing > 6 {
        return false; // fall too far to survive
    }
    // Open the floor cap and let physics drop the bot into the cave.
    for dy in 1..=cap {
        let n = name_at(bot, x, feet - dy, z);
        if is_solid(&n) && n != "obsidian" {
            dig_at(bot, x, feet - dy, z).await;
        }
    }
    bot.wait_ticks(16).await.ok();
    bot.entity.position.y < start_y - 1.0
}

/// Raise the bot's feet to `target_y` by sneaking, looking down, and placing a
/// block underneath each jump. Sneaking stops it walking off the 1-wide pillar.
pub(crate) async fn pillar_up(bot: &mut Bot<'_>, target_y: i32) -> bool {
    bot.set_control_state("sneak", true);
    let cell_x = bot.entity.position.x.floor() as i32;
    let cell_z = bot.entity.position.z.floor() as i32;
    for _ in 0..24 {
        if feet_y(bot) >= target_y {
            break;
        }
        walk_to_xz(bot, cell_x as f64 + 0.5, cell_z as f64 + 0.5, 0.1, 24).await;
        bot.set_control_state("sneak", true);
        let f = feet_y(bot);
        // Clear the climb path two/three blocks up so a stray block doesn't block the jump.
        for dy in [2, 3] {
            let n = name_at(bot, cell_x, f + dy, cell_z);
            if is_solid(&n) && n != "obsidian" {
                dig_at(bot, cell_x, f + dy, cell_z).await;
            }
        }
        if !select_item(bot, build_block(bot)).await.unwrap_or(false) {
            break;
        }
        bot.look_at(vec3(cell_x as f64 + 0.5, (f - 2) as f64, cell_z as f64 + 0.5));
        bot.wait_ticks(3).await.ok();
        bot.set_control_state("jump", true);
        bot.wait_ticks(7).await.ok();
        // Place on top of the block one below our feet (the pillar we stand on).
        if solid_at(bot, cell_x, f - 1, cell_z) {
            let _ = bot.place_block(cell_x, f - 1, cell_z, Face::Top).await;
        }
        bot.wait_ticks(5).await.ok();
        bot.set_control_state("jump", false);
        bot.wait_ticks(8).await.ok();
    }
    feet_y(bot) >= target_y
    // Leave sneak ON — caller clears it once the block is poured.
}

/// The terrain surface height AROUND the bot (not its own column — that may be the
/// open mining shaft it's standing at the bottom of). Scans a few neighbour columns
/// downward from the sky and returns the highest solid top found, or None if the
/// bot is on/near the surface already.
fn surrounding_surface_y(bot: &Bot) -> Option<i32> {
    let p = bot.entity.position;
    let (cx, cz) = (p.x.floor() as i32, p.z.floor() as i32);
    let mut best: Option<i32> = None;
    for (dx, dz) in [(3, 0), (-3, 0), (0, 3), (0, -3), (3, 3), (-3, -3)] {
        for y in (feet_y(bot) + 1..=200).rev() {
            if solid_at(bot, cx + dx, y, cz + dz) {
                best = Some(best.map_or(y, |b| b.max(y)));
                break;
            }
        }
    }
    best
}

/// Climb out of a deep mining shaft back to daylight. After mining iron a bot ends
/// up dozens of blocks down at the bottom of a 1-wide shaft; the A* pathfinder can't
/// scaffold up it, so surface-needing tasks (water) roam in place forever. Pillar up
/// the open shaft to the surrounding terrain height so those tasks start from grass.
/// No-op (returns true) when the bot is already at/near the surface.
pub(crate) async fn climb_out_of_pit(bot: &mut Bot<'_>) -> bool {
    // A dry shaft only. In water, the pillar digs the roof by hand while submerged (one dig blocks
    // for minutes): race i4 rust-race-003 sat in this for 279 s at gather_wood's start and drowned,
    // and water_roofed hung its 90 s here. Leaving water is `leave_water`'s job.
    if crate::bot_utils::head_in_water(bot) || crate::bot_utils::feet_in_water(bot) || bot.breath_alarm {
        return false;
    }
    let Some(surface) = surrounding_surface_y(bot) else {
        return true; // already at the surface (nothing solid overhead nearby)
    };
    if feet_y(bot) >= surface - 2 {
        return true;
    }
    let target = surface + 1;
    // pillar_up caps at 24 blocks/call; loop until we reach the target or stall.
    for _ in 0..4 {
        let before = feet_y(bot);
        pillar_up(bot, target).await;
        bot.set_control_state("sneak", false);
        let now = feet_y(bot);
        if now >= surface - 1 || now <= before {
            break;
        }
    }
    feet_y(bot) >= surface - 2
}

// ── fluids ──────────────────────────────────────────────────────────────────

/// Nearest visible fluid source, preferring one with air directly above.
fn find_fluid(bot: &Bot, fluid: &str, max_dist: i32) -> Option<(i32, i32, i32)> {
    let mut positions = bot.find_exposed_blocks(fluid, max_dist, 64);
    // Never treat the FRAME'S OWN CUPS as a pool. A stale pour leaves lava sitting in a frame cell;
    // on resume, prepare picked it as "the lava" (live: `lava=Some((901,-51,585))` = the top-right
    // column cup), anchored a bogus cast-site chamber 3 blocks off the real frame at (895,-50,585),
    // and the bot died/left. The cast handles a lava-filled cup itself (skip pour → water → obsidian),
    // so the pool-finder must simply skip the frame plane whenever a frame anchor exists.
    if fluid == "lava" {
        let anchor = *FRAME_ANCHOR.lock().unwrap();
        if let Some((bx, by, bz)) = anchor {
            positions.retain(|&(x, y, z)| !(z == bz && (bx..=bx + 3).contains(&x) && (by..=by + 4).contains(&y)));
        }
        positions.retain(|&q| !lava_retired(q));
    }
    if positions.is_empty() {
        return None;
    }
    let with_air: Vec<_> = positions
        .iter()
        .copied()
        .filter(|&(x, y, z)| is_air(&name_at(bot, x, y + 1, z)))
        .collect();
    let pick = if with_air.is_empty() { positions } else { with_air };
    let o = bot.entity.position;
    pick.into_iter().min_by(|a, b| {
        let da = (a.0 as f64 - o.x).powi(2) + (a.1 as f64 - o.y).powi(2) + (a.2 as f64 - o.z).powi(2);
        let db = (b.0 as f64 - o.x).powi(2) + (b.1 as f64 - o.y).powi(2) + (b.2 as f64 - o.z).powi(2);
        da.partial_cmp(&db).unwrap()
    })
}

/// Stand beside a fluid source and fill an empty bucket from it.
pub(crate) async fn fill_bucket(bot: &mut Bot<'_>, fluid: &str) -> bool {
    cast_debug(&format!(
        "fill {fluid}: ENTER empty_buckets={} {fluid}_buckets={}",
        count_items(bot, "bucket"),
        count_items(bot, &format!("{fluid}_bucket"))
    ));
    if count_items(bot, "bucket") < 1 {
        return false;
    }
    // Find an EDGE source (open surface + a solid horizontal neighbour to stand on),
    // not just the nearest — the centre of a pool has only fluid neighbours, so the
    // bot would have nowhere safe to stand. find_blocks returns nearest-first.
    // Settle first + retry: a just-dug chamber's block updates can leave the local
    // world momentarily missing the pool we located a moment ago.
    // Wide + high cap: standing at a big pool's edge, the nearest blocks are all interior
    // lava (fluid-only neighbours, no stand spot); the scoopable rim blocks are farther down
    // the nearest-first list, so a small cap (64) never reaches them. Match prepare's range.
    let mut candidates = bot.find_exposed_blocks(fluid, 24, 256);
    if candidates.is_empty() {
        bot.wait_ticks(10).await.ok();
        candidates = bot.find_exposed_blocks(fluid, 24, 256);
    }
    // Only SOURCE blocks (level=0) can be bucketed — flowing edges scoop nothing. Try
    // source blocks first (stable sort keeps nearest-first within each group).
    candidates.sort_by_key(|&(x, y, z)| u8::from(!is_fluid_source(bot, x, y, z, fluid)));
    let dirs8 = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
    // Classify stand options against SOURCE blocks only (flowing scoops nothing). A FLUSH
    // stand puts the feet ONE block above the source — a shallow look that scoops reliably.
    // A RECESSED stand (feet TWO above) makes the look too steep and misses (observed live:
    // bot at y=-37 over a y=-39 source failed all rounds). So prefer, in order: a natural
    // flush stand; recessed; a roofed source (dig its roof). The pillar-beside-a-source fallback was
    // removed (jumping in place next to the lake killed rust-gym-003 right after a good scoop). Building a
    // flush stand was removed — its goto routed the bot over the sea (hp=0 under the surface).
    let mut flush: Option<((i32, i32, i32), (f64, f64, f64))> = None;
    let mut recessed: Option<((i32, i32, i32), (f64, f64, f64))> = None;
    let mut any_source: Option<(i32, i32, i32)> = None; // nearest source w/ air above (pillar fallback)
    // Sources ROOFED by diggable rock (the deep lakes at y≈-55 sit under a deepslate ceiling; the
    // bot's tunnel meets them from the side): (source, roof block). Scooped by digging the roof.
    let mut covered: Vec<((i32, i32, i32), (i32, i32, i32))> = Vec::new();
    let mut source_count = 0u32;
    for src in candidates {
        if !is_fluid_source(bot, src.0, src.1, src.2, fluid) || OBSIDIAN_BLOCKED.lock().unwrap().contains(&src) {
            continue; // only ever anchor the scoop on a real, reachable source block
        }
        let above = name_at(bot, src.0, src.1 + 1, src.2);
        if !is_air(&above) {
            // Bedrock is not a diggable roof: race i6's rust-race-003 spent 3 h "digging roof … (bedrock)"
            // over a −62 pool, 39 times, without one scoop.
            if fluid == "lava" && is_solid(&above) && above != "obsidian" && above != "bedrock" && covered.len() < 8 {
                covered.push((src, (src.0, src.1 + 1, src.2)));
            }
            continue; // need an open surface to scoop (or dig one — see `covered`)
        }
        source_count += 1;
        if any_source.is_none() {
            any_source = Some(src);
        }
        for (dx, dz) in dirs8 {
            let (sx, sz) = (src.0 + dx, src.2 + dz);
            if flush.is_none()
                && solid_at(bot, sx, src.1, sz)
                && is_air(&name_at(bot, sx, src.1 + 1, sz))
                && is_air(&name_at(bot, sx, src.1 + 2, sz))
            {
                flush = Some((src, (sx as f64 + 0.5, (src.1 + 1) as f64, sz as f64 + 0.5)));
            }
            if recessed.is_none()
                && solid_at(bot, sx, src.1 + 1, sz)
                && is_air(&name_at(bot, sx, src.1 + 2, sz))
                && is_air(&name_at(bot, sx, src.1 + 3, sz))
            {
                recessed = Some((src, (sx as f64 + 0.5, (src.1 + 2) as f64, sz as f64 + 0.5)));
            }
        }
        if flush.is_some() {
            break; // best option found — nearest-first, so stop here
        }
    }
    // Resolve the preference. Water doesn't burn, so as a last resort scoop it from
    // directly above; never do that for lava.
    let (src, stand) = if let Some(f) = flush {
        f
    } else if let Some(r) = recessed {
        r
    } else if fluid == "lava" && !covered.is_empty() {
        // ROOFED LAKE: no source has air above, but sources under diggable rock are right here
        // (natural: "0 sources, 256 lava blocks seen", deepslate at src+1). Stand on a solid
        // horizontal neighbour at the source's level (the tunnel floor — feet one above the
        // source, the flush geometry), dig the roof block, then scoop as usual.
        let dirs8 = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
        let mut pick: Option<((i32, i32, i32), (i32, i32, i32), (i32, i32))> = None;
        'outer: for &(s, roof) in &covered {
            for (dx, dz) in dirs8 {
                let (nx, nz) = (s.0 + dx, s.2 + dz);
                // Two-deep solid floor, not a falling block: a one-block shelf over the lake (or
                // gravel) gave way under rust-gym-002 — stand (1488,-54,1408), bot found at y −59 in
                // the lake after the roof dig (04:31 binary).
                let floor = name_at(bot, nx, s.1, nz);
                let under = name_at(bot, nx, s.1 - 1, nz);
                if solid_at(bot, nx, s.1, nz)
                    && !is_lava(&floor)
                    && !floor.contains("gravel")
                    && !floor.contains("sand")
                    && is_solid(&under)
                    && !is_lava(&under)
                    && is_air(&name_at(bot, nx, s.1 + 1, nz))
                    && is_air(&name_at(bot, nx, s.1 + 2, nz))
                {
                    pick = Some((s, roof, (nx, nz)));
                    break 'outer;
                }
            }
        }
        if let Some((s, roof, (nx, nz))) = pick {
            bot.movement.blocks_cant_break.clear();
            let _ = bot.goto_near(nx, s.1 + 1, nz, 1.0).await;
            walk_to_xz(bot, nx as f64 + 0.5, nz as f64 + 0.5, 0.4, 40).await;
            cast_debug(&format!("fill lava: roofed source {s:?} — digging roof {roof:?} ({}) from ({nx},{},{nz})", name_at(bot, roof.0, roof.1, roof.2), s.1 + 1));
            dig_at(bot, roof.0, roof.1, roof.2).await;
            bot.wait_ticks(4).await.ok();
            (s, (nx as f64 + 0.5, (s.1 + 1) as f64, nz as f64 + 0.5))
        } else {
            // ON THE ROOF (feet = source+2, standing on the roof itself): dig the roof of a source
            // 1.5–2.6 blocks away horizontally — never the block under our feet — and scoop from
            // where we stand. Closer than ~1.5 the down-ray clips the top of our own block.
            let p = bot.entity.position;
            let fy = feet_y(bot);
            // …or IN THE TUNNEL beside a roofed lake (feet = source+1 on solid ground, source ≤2.3
            // away but not an 8-neighbour): dig that source's roof — it is at our feet level, and
            // lava never flows up — then scoop from here. rust-gym-00x (04:31 binary) refused
            // `5 roofed sources but no stand (feet=-54)` ×5 at (981,-55,1488) and retired the pool.
            let far = covered.iter().find(|&&(s, _)| {
                let d = ((s.0 as f64 + 0.5 - p.x).powi(2) + (s.2 as f64 + 0.5 - p.z).powi(2)).sqrt();
                (fy == s.1 + 2 && (1.5..=2.6).contains(&d))
                    || (fy == s.1 + 1 && d <= 2.3 && solid_at(bot, p.x.floor() as i32, fy - 1, p.z.floor() as i32))
            });
            let Some(&(s, roof)) = far else {
                // No stand from HERE: move toward the nearest roofed source (the pathfinder digs
                // through rock and refuses lava-adjacent cells) so the caller's next attempt scoops
                // from its side. Refusing in place starved the mold: rust-gym-002 (05:01 binary)
                // `5 roofed sources but no stand (feet=-54)` ×4 on its pad → `MOLD end 4/10`.
                // Only sources BELOW our feet: climbing toward an elevated one digs up into its
                // flow — rust-gym-002 (10:45 binary) `moving toward (1165,-49,439)` from feet -54,
                // dug up, `eat: health 0->0`, `MOLD end 4/10`.
                if let Some(&(s, _)) = covered.iter().filter(|(s, _)| s.1 < fy).min_by(|a, b| {
                    let da = (a.0 .0 as f64 + 0.5 - p.x).powi(2) + (a.0 .2 as f64 + 0.5 - p.z).powi(2);
                    let db = (b.0 .0 as f64 + 0.5 - p.x).powi(2) + (b.0 .2 as f64 + 0.5 - p.z).powi(2);
                    da.total_cmp(&db)
                }) {
                    // DIG DOWN IN PLACE to the roof (feet = source+2) when our own column is plain rock
                    // touching no lava and the cell at source+1 is solid to stand on. The goto below asks
                    // the pathfinder to dig beside a lava body, which it refuses (lava-adjacent cells):
                    // batch 6 had three bots at feet −51/−52 over a −56 roofed lake, each repeating
                    // `moving toward …` for 20+ min without descending. The next attempt then hits the
                    // on-roof scoop above (feet source+2, source 1.5–2.6 away).
                    let (px, pz) = (p.x.floor() as i32, p.z.floor() as i32);
                    let lava_touch = |y: i32| {
                        [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(ox, oz)| is_lava(&name_at(bot, px + ox, y, pz + oz)))
                    };
                    if fy > s.1 + 2 && solid_at(bot, px, s.1 + 1, pz) && !(s.1 + 1..fy).any(lava_touch) {
                        cast_debug(&format!("fill lava: roofed lake below (feet={fy}, source {}) — digging down in place to the roof (feet {})", s.1, s.1 + 2));
                        descend_to_y(bot, s.1 + 2).await;
                        return false;
                    }
                    cast_debug(&format!("fill lava: {} roofed sources, no stand from (feet={fy}) — moving toward {s:?}", covered.len()));
                    bot.movement.blocks_cant_break.clear();
                    // Above the roof (feet ≥ source+2): walk ALONG the roof to within the on-roof
                    // scoop range (feet source+2). Aiming at source+1 (beside it, under the roof)
                    // was unreachable from above — rust-gym-001: 4× `moving toward (1291,-58,821)`
                    // from feet -55/-56 in one second, no movement, `MOLD end 2/10`.
                    let target_feet = if fy >= s.1 + 2 { s.1 + 2 } else { s.1 + 1 };
                    let _ = bot.goto_near(s.0, target_feet, s.2, 2.0).await;
                } else {
                    cast_debug(&format!("fill lava: {} roofed sources but no stand (feet={fy}) — refuse", covered.len()));
                }
                return false;
            };
            cast_debug(&format!("fill lava: roofed source {s:?} from ON the roof — digging {roof:?} ({})", name_at(bot, roof.0, roof.1, roof.2)));
            dig_at(bot, roof.0, roof.1, roof.2).await;
            bot.wait_ticks(4).await.ok();
            (s, (p.x, fy as f64, p.z))
        }
    } else if fluid == "lava" {
        cast_debug(&format!(
            "fill lava: NO scoopable source ({source_count} sources, {} lava blocks seen)",
            bot.find_exposed_blocks(fluid, 24, 256).len()
        ));
        return false;
    } else {
        let Some(s) = find_fluid(bot, fluid, 16) else {
            return false;
        };
        (s, (s.0 as f64 + 0.5, (s.1 + 1) as f64, s.2 as f64 + 0.5))
    };
    let filled_bucket_name = format!("{fluid}_bucket");
    // Success = the filled-bucket count ROSE. The old `> 0` check reported "OK" on every refill
    // once a single lava bucket was held, so natural-lake refills silently failed (gym-002: four
    // "OK (round 0)" lines, lava_b stayed 1) and the mold ran out of lava on the top row.
    let before = count_items(bot, &filled_bucket_name);
    // Up to 3 re-approach rounds: navigate to the stand spot, then try scooping any
    // reachable source block (the located one OR a neighbour) from a couple of aim
    // heights. Being a block off the exact stand spot is fine — the source is well
    // within reach; we just need the look to actually land on lava.
    for round in 0..3 {
        // SURVIVAL: at a deep lava SEA the stand can end up on/over the lava (no solid rim
        // to stand on), and the bot takes fire damage while repeatedly failing to scoop —
        // it burned to death at y-54 casting, losing its whole portal kit and regressing to
        // Mine Iron. If health is low, bail UP off the lava with the kit intact rather than
        // die: pillar a few blocks up, then abort this fill so the step retries alive.
        // 14, not 8: lava fire keeps burning ~1 hp/s for seconds after the bot leaves the lava, so a
        // round started at 7–10 hp is already lost (batch A walk-cap build: both refill deaths entered
        // their last round at hp ≤ 7). Retreat and heal with enough margin to outlive the fire.
        if bot.health < 14.0 {
            let p = bot.entity.position;
            // DEAD (hp<=0): a corpse can't pillar/goto/eat — those block until the 1800s step
            // watchdog kills the whole cast (011 hung here after 2 obsidian, made 0 more for
            // 30min). Respawn and abort so the step re-derives; keep_inventory preserves the kit
            // and the already-placed obsidian persists, so the frame resumes.
            if bot.health <= 0.0 {
                cast_debug("fill lava: DEAD — respawning (a corpse can't retreat; this hung the cast)");
                respawn_at_frame(bot).await;
                bot.set_control_state("sneak", false);
                return false;
            }
            cast_debug(&format!("fill lava: ABORT low health={:.0} at ({:.1},{:.1},{:.1}) below={} — retreat + heal", bot.health, p.x, p.y, p.z, name_at(bot, p.x.floor() as i32, p.y.floor() as i32 - 1, p.z.floor() as i32)));
            // Retreat CLEAR of the lava, then HEAL before returning. The old code just pillared
            // 4 up and returned, so the step re-dived at ~0 hp and burned again — casting 0
            // obsidian across dozens of retries (the bot ate 18x but FIRE, not hunger, was the
            // killer: it keeps burning on solid ground after a lava nick, and eat_if_hurt won't
            // even eat within 3 of lava). Pillar clear so the fire burns out (a few seconds off
            // the lava) and eating is allowed, then top health back up so the NEXT cast approach
            // starts survivable instead of at death's door.
            pillar_up(bot, feet_y(bot) + 6).await;
            bot.set_control_state("sneak", false);
            for &(dx, dz) in &[(1, 0), (0, 1), (-1, 0), (0, -1)] {
                if !raw_lava_near(bot, 3) {
                    break;
                }
                let q = bot.entity.position;
                let _ = bot.goto_xz(q.x.floor() as i32 + dx * 4, q.z.floor() as i32 + dz * 4, 2.0).await;
            }
            for _ in 0..8 {
                // Died during the retreat → respawn now; eating as a corpse burned 70 s
                // (rust-gym-003: `eat: health 0->0` ×8 after `ABORT low health=4`).
                if bot.health <= 0.0 {
                    respawn_at_frame(bot).await;
                    break;
                }
                if bot.health >= 16.0 {
                    break;
                }
                eat_if_hurt(bot).await;
                bot.wait_ticks(20).await.ok();
            }
            return false;
        }
        // PRE-DESCENT SAFETY — the #1 death, and the sole wall past block 1: the descend/approach
        // below walks the bot DOWN toward the source through the open lava band; it burns to 0
        // DURING that nav and is only caught at the NEXT round's top guard ("DEAD — respawning"),
        // making zero progress past block 1 (frame stuck at 1/10, deaths climbing). Every fire-safety
        // guard + the cap sit AFTER the descent, too late. So BEFORE stepping into the band:
        //  (1) heal to full while still clear of lava (eat_if_hurt self-guards the 3-block radius),
        //      so the descent starts survivable instead of at death's door, and
        //  (2) cap the lava the bot is standing over/beside RIGHT NOW (its own 3×3×2 footing) so the
        //      first steps of the descent are on cobble, not fire. Never cap the source we scoop.
        for _ in 0..3 {
            if bot.health >= 18.0 {
                break;
            }
            eat_if_hurt(bot).await;
        }
        if count_items(bot, "cobblestone") >= 1 {
            let p = bot.entity.position;
            let (px, pz, fy) = (p.x.floor() as i32, p.z.floor() as i32, feet_y(bot));
            for dx in -1..=1 {
                for dz in -1..=1 {
                    for dyy in [-1_i32, 0] {
                        let (cx, cy, cz) = (px + dx, fy + dyy, pz + dz);
                        // Keep the scoop SOURCE and its whole horizontal ring open: fill_bucket
                        // scoops src OR any horizontal neighbour of it (4 targets), so capping any
                        // cell within 1 of src turns a scoop candidate into cobblestone and the
                        // fill fails ("src=…(cobblestone), all rounds failed" — the bot caps the
                        // very lava it needs). Only cap the surrounding FIELD, never the scoop lane.
                        if (cx - src.0).abs() <= 1 && (cz - src.2).abs() <= 1 {
                            continue;
                        }
                        if is_lava(&name_at(bot, cx, cy, cz)) {
                            place_cobble(bot, (cx, cy, cz)).await;
                        }
                    }
                }
            }
        }
        let _ = bot.goto_near(stand.0 as i32, stand.1 as i32, stand.2 as i32, 1.0).await;
        walk_to_xz(bot, stand.0, stand.2, 0.15, 50).await; // centred: the body must stay clear of the source column (see the touch guard)
        // DESCEND to the source's level if the stand nav left the bot perched ABOVE it. A scoop is a
        // short raycast; when the source sits well below the bot (e.g. the bot was tp'd to y-41 but
        // the scoopable source is in the deep sea at y-55) goto_near stops at the higher lip and the
        // bot scoops from 14 blocks up (`hdist=14.9 lava true->lava` → "all rounds failed" → retire).
        // Drop to one above the source and re-pin, so the aim is a shallow reach into the surface.
        if feet_y(bot) > src.1 + 2 {
            bot.movement.blocks_cant_break.clear();
            descend_to_y(bot, src.1 + 1).await;
            let _ = bot.goto_near(stand.0 as i32, (src.1 + 1).max(feet_y(bot)), stand.2 as i32, 1.0).await;
            walk_to_xz(bot, stand.0, stand.2, 0.15, 40).await;
        }
        if !select_item(bot, "bucket").await.unwrap_or(false) {
            return false;
        }
        // Candidate source blocks: the located one + its horizontal neighbours that
        // are actually this fluid (so a slightly-off bot still has a target it sees).
        // Target SOURCE blocks (level=0) — the located one and any source neighbour, at
        // the source level and one below (source lava often sits a level down from what the
        // exposed scan reported). Flowing blocks are skipped; fall back to src if none found.
        let mut targets: Vec<(i32, i32, i32)> = Vec::new();
        for (dx, dy, dz) in [(0, 0, 0), (1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, -1, 0)] {
            let t = (src.0 + dx, src.1 + dy, src.2 + dz);
            if is_fluid_source(bot, t.0, t.1, t.2, fluid) && !OBSIDIAN_BLOCKED.lock().unwrap().contains(&t) {
                targets.push(t);
            }
        }
        if targets.is_empty() {
            if OBSIDIAN_BLOCKED.lock().unwrap().contains(&src) {
                // Every scoop ray from here ends in obsidian — a new stand/source is needed, which
                // the caller's next fill_bucket picks (blocked sources are skipped as candidates).
                cast_debug(&format!("fill {fluid}: {src:?} and its neighbours are obsidian-blocked — re-pick"));
                return false;
            }
            targets.push(src);
        }
        {
            let p = bot.entity.position;
            let d = ((src.0 as f64 - p.x).powi(2) + (src.2 as f64 - p.z).powi(2)).sqrt();
            let lvl = bot.block_at(src.0, src.1, src.2).and_then(|b| b.properties.get("level").cloned());
            cast_debug(&format!(
                "fill try r{round}: src={src:?}({}) lvl={lvl:?} targets={} bot=({:.1},{:.1},{:.1}) hdist={d:.1} hp={:.0}",
                name_at(bot, src.0, src.1, src.2), targets.len(), p.x, p.y, p.z, bot.health
            ));
        }
        // CLOSE THE GAP: a scoop is a raycast from the eyes, so being >~2 blocks from the source
        // makes the aim miss and the bucket never fills (`hdist=8.6 lava true->lava` → "all rounds
        // failed" → retire loop, on wide lava lakes where goto_near/stand stopped short). Before
        // scooping, step onto a non-lava cell right beside the nearest target so the source is in
        // arm's reach. Pick the target closest to the bot and a safe (solid floor + air) neighbour.
        {
            let p = bot.entity.position;
            let nearest = targets.iter().min_by(|a, b| {
                let da = (a.0 as f64 - p.x).powi(2) + (a.2 as f64 - p.z).powi(2);
                let db = (b.0 as f64 - p.x).powi(2) + (b.2 as f64 - p.z).powi(2);
                da.total_cmp(&db)
            });
            if let Some(&(tx, ty, tz)) = nearest {
                let hdist = ((tx as f64 - p.x).powi(2) + (tz as f64 - p.z).powi(2)).sqrt();
                if hdist > 2.0 {
                    // A horizontal neighbour of the source with solid footing + 2 air to stand in,
                    // and itself not lava — stand there and scoop across into the source.
                    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let (sx, sz) = (tx + dx, tz + dz);
                        if !is_lava(&name_at(bot, sx, ty, sz))
                            && !is_lava(&name_at(bot, sx, ty + 1, sz))
                            && solid_at(bot, sx, ty - 1, sz)
                            && is_air(&name_at(bot, sx, ty, sz))
                            && is_air(&name_at(bot, sx, ty + 1, sz))
                        {
                            bot.movement.blocks_cant_break.clear();
                            let _ = bot.goto_near(sx, ty, sz, 1.0).await;
                            walk_to_xz(bot, sx as f64 + 0.5, sz as f64 + 0.5, 0.4, 40).await;
                            break;
                        }
                    }
                }
            }
        }
        for t in &targets {
            // UNOBSTRUCT the fluid raytrace: filling a bucket ray-traces from the eye to the
            // lava SURFACE. If the block directly above the source is solid (common when the
            // bot tunnels to a source at the band — the ceiling stays), the ray hits that block
            // instead of the fluid and activate scoops nothing (live: held=bucket, "lava
            // true->lava" 373×, perfect geometry). Open the column above the source first.
            // Never scoop a source at or above our own feet level — that lava is IN our body layer
            // and flows into our cell. A pool with sources at two levels (-56 and -55) had prepare
            // drop the bot to -55 for the -56 surface; the fill then picked a -55 source beside the
            // feet and the bot burned (rust-gym-002: hp 20 → 3 in 4 s, pool retired).
            if t.1 >= feet_y(bot) {
                cast_debug(&format!("  source {t:?} is at/above our feet ({}) — skip", feet_y(bot)));
                continue;
            }
            // Never scoop a source in a column our body touches (half-width 0.3, +0.1 drift margin):
            // scooping turns it into a HOLE at lava level right at our feet, and the next drift drops
            // us in with the neighbours flowing in — the "fell below the source within 3 s" deaths.
            // rust-gym-002: at x=441.7 it scooped (442,-55,555) and (442,-55,556), then was found at
            // (442.6,-55.6,555.7) hp 14 → dead.
            {
                let p = bot.entity.position;
                let touches = [(-0.4, -0.4), (-0.4, 0.4), (0.4, -0.4), (0.4, 0.4)]
                    .iter()
                    .any(|&(ox, oz)| ((p.x + ox).floor() as i32, (p.z + oz).floor() as i32) == (t.0, t.2));
                if touches {
                    cast_debug(&format!("  source {t:?} is in a column our body touches — skip"));
                    continue;
                }
            }
            let above = name_at(bot, t.0, t.1 + 1, t.2);
            // Never the block under the bot's own body: rust-gym-002 stood ON the source's roof
            // (298,-55,549), dug it (`unobstruct above source … → dig`) and dropped into the lava.
            let under_body = {
                let p = bot.entity.position;
                let fy = feet_y(bot);
                t.1 + 1 == fy - 1
                    && [(-0.299, -0.299), (-0.299, 0.299), (0.299, -0.299), (0.299, 0.299)]
                        .iter()
                        .any(|&(ox, oz)| ((p.x + ox).floor() as i32, (p.z + oz).floor() as i32) == (t.0, t.2))
            };
            if under_body {
                cast_debug(&format!("  source {t:?} is under our own feet — skip"));
                continue;
            }
            if above == "bedrock" {
                cast_debug(&format!("  source {t:?} under bedrock — skip"));
                continue;
            }
            if is_solid(&above) {
                cast_debug(&format!("  unobstruct above source {t:?}: {above} → dig"));
                dig_at(bot, t.0, t.1 + 1, t.2).await;
            }
            // APPROACH: a bucket fill ray-traces only ~4.5 blocks. After casting a few frame
            // blocks the bot drifts and the next source ends up 5-6.5 away — the ray falls short
            // and the fill misses (live: reach=5.4..6.5 → "lava true->lava", stalled 011 at 3
            // obsidian). Step adjacent (to the safe air ABOVE the source) before scooping.
            {
                let p = bot.entity.position;
                let horiz = ((t.0 as f64 + 0.5 - p.x).powi(2) + (t.2 as f64 + 0.5 - p.z).powi(2)).sqrt();
                if horiz > 2.5 {
                    // Approach a concrete SAFE STAND beside the source — solid non-lava floor at the
                    // source's level, body cells air and not lava, no lava under the floor — not
                    // "near the air above the source" (range 1.8): that goal sits over the lake and
                    // the bot finished standing ON it (rust-gym-003, 01:53 binary: `FOOTING on lava
                    // at (401.7,-54,872.3)` → hp 0, refill death at 4/10). No safe stand → skip t.
                    // Flush stand (floor at the source level) first, else RECESSED (floor one above,
                    // feet source+2 — the stand scan's second choice). Flush-only skipped every
                    // target in a basin pool (rust-gym-002: `no safe stand beside it — skip` ×16,
                    // refill 0 buckets, mold ended at 2/10).
                    let stand_at = |floor_y: i32| {
                        [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)]
                            .iter()
                            .map(|&(dx, dz)| (t.0 + dx, t.2 + dz))
                            .filter(|&(sx, sz)| {
                                let floor = name_at(bot, sx, floor_y, sz);
                                is_solid(&floor)
                                    && !is_lava(&floor)
                                    && !is_lava(&name_at(bot, sx, floor_y - 1, sz))
                                    && is_air(&name_at(bot, sx, floor_y + 1, sz))
                                    && is_air(&name_at(bot, sx, floor_y + 2, sz))
                            })
                            .min_by(|a, b| {
                                let da = (a.0 as f64 + 0.5 - p.x).powi(2) + (a.1 as f64 + 0.5 - p.z).powi(2);
                                let db = (b.0 as f64 + 0.5 - p.x).powi(2) + (b.1 as f64 + 0.5 - p.z).powi(2);
                                da.total_cmp(&db)
                            })
                            .map(|(sx, sz)| (sx, sz, floor_y + 1))
                    };
                    let stand = stand_at(t.1).or_else(|| stand_at(t.1 + 1));
                    let Some((sx, sz, stand_feet)) = stand else {
                        cast_debug(&format!("  approach source {t:?}: {horiz:.1} away, no safe stand beside it — skip"));
                        continue;
                    };
                    cast_debug(&format!("  approach source {t:?}: {horiz:.1} away → stand ({sx},{stand_feet},{sz})"));
                    let _ = bot.goto_near(sx, stand_feet, sz, 0.8).await;
                    // No dig-toward fallback: digging a straight feet+head path toward the stand opened
                    // pool rims (rust-gym-002: three approaches of 66/126/100 s, the last ending at
                    // y -58 inside the lake). If the pathfinder can't reach the stand, the reach
                    // check below skips this target.
                    // The raw walk only finishes a SHORT last stretch (≤ 3 blocks). Since walk_to_xz counts
                    // real ticks it walks for real; when the pathfinder stopped 12 blocks short it crossed
                    // that gap in a straight line toward a stand beside the lake, and died (batch 10:
                    // `approach … 12.4 away`, `out of reach (12.8)`, then a lava death).
                    let q = bot.entity.position;
                    if ((sx as f64 + 0.5 - q.x).powi(2) + (sz as f64 + 0.5 - q.z).powi(2)).sqrt() <= 3.0 {
                        walk_to_xz(bot, sx as f64 + 0.5, sz as f64 + 0.5, 0.4, 30).await;
                    }
                }
            }
            // OUT OF REACH after the approach → skip this target now. A bucket ray reaches ~4.5;
            // when the stand was unreachable the bot still fired 3 aims from 8.5 blocks
            // (`lava true->lava … reach=8.5`) and re-approached twice — one 2-bucket refill on
            // rust-gym-003 took ~12 min (1400 → 2143 s) and the trial ran out of clock at 4/10.
            {
                let p = bot.entity.position;
                let reach = ((t.0 as f64 + 0.5 - p.x).powi(2) + (t.1 as f64 + 0.9 - (p.y + 1.62)).powi(2) + (t.2 as f64 + 0.5 - p.z).powi(2)).sqrt();
                if reach > 4.6 {
                    // Remember it: the next fill_bucket then picks OTHER sources in its 24-block scan
                    // instead of re-approaching the same unreachable one (rust-gym-002 timed out at
                    // 7/10 cycling `out of reach … — skip` on one pool).
                    cast_debug(&format!("  scoop t={t:?}: out of reach ({reach:.1}) after the approach — skip (blocked from now on)"));
                    OBSIDIAN_BLOCKED.lock().unwrap().push(*t);
                    continue;
                }
            }
            // FIRE-SAFETY: cap OPEN lava blocks touching the stand (except the target source)
            // with cobble so the bot isn't cooked by a lava FIELD while scooping — the open-band
            // fire (hp 20→0 faster than the bail) capped the cast at ~1 obsidian per death. One
            // adjacent source is fine (that's what we scoop); it's the surrounding field that kills.
            if count_items(bot, "cobblestone") >= 1 {
                let p = bot.entity.position;
                let fy = feet_y(bot);
                let (px, pz) = (p.x.floor() as i32, p.z.floor() as i32);
                for dx in -1..=1 {
                    for dz in -1..=1 {
                        for dyy in [-1_i32, 0] {
                            let (cx, cy, cz) = (px + dx, fy + dyy, pz + dz);
                            // Keep EVERY scoop target open, not just the current `t`: `targets` holds
                            // src + its scoopable neighbours, and capping any of them (while this loop
                            // iterates a different one) turns a source into cobblestone → the next
                            // round scoops a capped cell and "all rounds failed" forever. Cap only the
                            // field OUTSIDE the target set.
                            if targets.contains(&(cx, cy, cz)) {
                                continue; // keep the scoop hole(s) open
                            }
                            if is_lava(&name_at(bot, cx, cy, cz)) {
                                place_cobble(bot, (cx, cy, cz)).await;
                            }
                        }
                    }
                }
            }
            for dy in [0.6_f64, 0.2, 0.9] {
                // FOOTING guard (health-independent, checked BEFORE every activate): never scoop
                // while standing on lava. A receding rim / flowing-lava backfill can park the bot
                // on a source; the health bail below only fires AFTER the nick lands (too late —
                // 20→0 in ~1s). Bail the moment the block under our feet is lava, before damage.
                // Checks EVERY column the 0.6-wide body overlaps (a bot at x=402.0 straddles 401 and
                // 402). The retreat is a WALK to the nearest cell centre whose floor and body cells
                // are lava-free — not `pillar_up`: jumping in place beside the lava column is what
                // killed rust-gym-003 three times at (402.0,-54,872.6) (its own floor was deepslate
                // on the server; the lava was the neighbour column its box overlapped).
                {
                    let p = bot.entity.position;
                    let fy = feet_y(bot);
                    // Danger = lava under the body's CENTRE, or lava at feet/head height in any column
                    // the box overlaps. A flush stand's neighbour column has the SOURCE one level
                    // below the feet — that is the scoop target, not a hazard (treating it as one
                    // refused 5 good scoops on rust-gym-001 at (801.7,-54,1174.7) and retired the pool).
                    // ±0.299: a box edge exactly on a block boundary does not overlap that block.
                    let cols: Vec<(i32, i32)> = [(-0.299, -0.299), (-0.299, 0.299), (0.299, -0.299), (0.299, 0.299)]
                        .iter()
                        .map(|&(ox, oz)| ((p.x + ox).floor() as i32, (p.z + oz).floor() as i32))
                        .collect();
                    let body_lava = |x: i32, z: i32| is_lava(&name_at(bot, x, fy, z)) || is_lava(&name_at(bot, x, fy + 1, z));
                    let lava_col = |x: i32, z: i32| is_lava(&name_at(bot, x, fy - 1, z)) || body_lava(x, z);
                    let (ccx, ccz) = (p.x.floor() as i32, p.z.floor() as i32);
                    if is_lava(&name_at(bot, ccx, fy - 1, ccz)) || cols.iter().any(|&(x, z)| body_lava(x, z)) {
                        let (cx, cz) = (p.x.floor() as i32, p.z.floor() as i32);
                        let safe = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)]
                            .iter()
                            .map(|&(dx, dz)| (cx + dx, cz + dz))
                            .find(|&(x, z)| {
                                solid_at(bot, x, fy - 1, z)
                                    && is_air(&name_at(bot, x, fy, z))
                                    && is_air(&name_at(bot, x, fy + 1, z))
                                    && [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)].iter().all(|&(ox, oz)| !lava_col(x + ox, z + oz))
                            });
                        cast_debug(&format!("fill {fluid}: FOOTING overlaps lava at ({:.1},{:.1},{:.1}) — stepping to {safe:?}, no scoop", p.x, p.y, p.z));
                        bot.set_control_state("sneak", false);
                        if let Some((sx, sz)) = safe {
                            walk_to_xz(bot, sx as f64 + 0.5, sz as f64 + 0.5, 0.2, 30).await;
                        } else {
                            // Standing IN the pool's top layer: every same-level neighbour touches
                            // lava. Climb one level instead — the mold pad / rim is there. rust-gym-002
                            // (11:51 binary) stood at −55 in a re-flooded scooped cell, `stepping to
                            // None`, and burned 10 s later (`eat: health 1->0`) at 4/10.
                            let up = (-2..=2)
                                .flat_map(|dx| (-2..=2).map(move |dz| (cx + dx, cz + dz)))
                                .filter(|&(x, z)| {
                                    solid_at(bot, x, fy, z)
                                        && is_air(&name_at(bot, x, fy + 1, z))
                                        && is_air(&name_at(bot, x, fy + 2, z))
                                        && !is_lava(&name_at(bot, x, fy, z))
                                })
                                .min_by_key(|&(x, z)| (x - cx).abs() + (z - cz).abs());
                            cast_debug(&format!("fill {fluid}: no same-level stand — climbing to {up:?}"));
                            if let Some((ux, uz)) = up {
                                bot.movement.blocks_cant_break.clear();
                                let _ = bot.goto_near(ux, fy + 1, uz, 0.5).await;
                            }
                        }
                        return false;
                    }
                }
                // Bail the INSTANT health drops — a receding-rim scoop can leave the bot standing
                // on a lava column, cooking it 20→0 in ~1s, FASTER than the once-per-round guard
                // above (that's how it died in-limbo at 8/10). Bail at <14 (only ~6 dmg taken) so
                // it retreats ALIVE with the kit; the caller re-scoops from a fresh, safer approach.
                if bot.health < 14.0 {
                    let p = bot.entity.position;
                    // DEAD (hp<=0): don't retreat a corpse — pillar/goto/eat block until the step
                    // watchdog kills the cast. Respawn + abort; kit + placed obsidian persist.
                    if bot.health <= 0.0 {
                        cast_debug("fill lava: EARLY BAIL DEAD — respawning (corpse can't retreat)");
                        respawn_at_frame(bot).await;
                        bot.set_control_state("sneak", false);
                        return false;
                    }
                    cast_debug(&format!("fill lava: EARLY BAIL hp={:.0} at ({:.1},{:.1},{:.1}) below={} — retreat + heal", bot.health, p.x, p.y, p.z, name_at(bot, p.x.floor() as i32, p.y.floor() as i32 - 1, p.z.floor() as i32)));
                    // Retreat CLEAR of the lava + HEAL, not just pillar 3 and return: the old
                    // code re-approached still hurt (and often still on fire) → took more damage
                    // → bailed again, never recovering (0/10 across attempts). Get off the lava
                    // column so fire burns out and eating is allowed, then top health back up so
                    // the next scoop/cast starts survivable. Same recovery as the <8 abort.
                    pillar_up(bot, feet_y(bot) + 6).await;
                    bot.set_control_state("sneak", false);
                    for &(dx, dz) in &[(1, 0), (0, 1), (-1, 0), (0, -1)] {
                        if !raw_lava_near(bot, 3) {
                            break;
                        }
                        let q = bot.entity.position;
                        let _ = bot.goto_xz(q.x.floor() as i32 + dx * 4, q.z.floor() as i32 + dz * 4, 2.0).await;
                    }
                    for _ in 0..8 {
                        // Died during the retreat → respawn now; eating as a corpse burned 70 s
                        // (rust-gym-003: `eat: health 0->0` ×8 after `ABORT low health=4`).
                        if bot.health <= 0.0 {
                            respawn_at_frame(bot).await;
                            break;
                        }
                        if bot.health >= 16.0 {
                            break;
                        }
                        eat_if_hurt(bot).await;
                        bot.wait_ticks(20).await.ok();
                    }
                    return false;
                }
                let was_lava = is_lava(&name_at(bot, t.0, t.1, t.2));
                let look_pt = vec3(t.0 as f64 + 0.5, t.1 as f64 + dy, t.2 as f64 + 0.5);
                bot.look_at(look_pt);
                bot.wait_ticks(7).await.ok();
                // RE-ASSERT the look immediately before activate: the 7-tick settle can reset
                // pitch to LEVEL (the movement-facing look at mod.rs:1638), so use_item would
                // carry a horizontal rotation and the server's fluid raytrace misses lava BELOW
                // the bot — scoops fine on the gym's horizontal lava, fails on the natural
                // downward scoop ("lava true->lava" 373×). look() sets pitch directly and
                // use_item embeds the current rotation, so re-looking here guarantees the steep
                // downward pitch is what gets sent.
                bot.look_at(look_pt);
                // ★ HOLD THE EMPTY BUCKET before scooping. The cap/floor placement earlier in this
                // loop (place_cobble) leaves COBBLESTONE selected, so activate_item "scoops" with
                // cobble and the lava stays lava (live: `held=cobblestone lava true->lava`, 0 fills
                // across every bot on the deep band — the whole 2026-09 nether wall after the
                // bucket-count fix). select_item is a no-op if the bucket is already held.
                let _ = select_item(bot, "bucket").await;
                let held = bot.held_item().map(|i| i.name.clone());
                let (sent_yaw, sent_pitch) = (bot.entity.yaw.to_degrees(), bot.entity.pitch.to_degrees());
                let eye = bot.entity.position;
                let reach = ((look_pt.x - eye.x).powi(2) + (look_pt.y - (eye.y + 1.62)).powi(2) + (look_pt.z - eye.z).powi(2)).sqrt();
                // LINE OF SIGHT. The scoop is a raycast, and from inside the bot's own 1-wide
                // tunnel a DIAGONAL target's ray clips the tunnel wall at head level before it
                // ever reaches the lava — every aim "missed" with the rotation, reach and source
                // all correct (natural: `lava true->lava` ×15 with deepslate at (1262,-53,435) and
                // (1261,-53,434) beside the head). Trace the ray through the local world and dig
                // the first solid cell in the way (never obsidian/lava), then re-aim.
                if let Some(occ) = first_solid_on_ray(bot, vec3(eye.x, eye.y + 1.62, eye.z), look_pt, *t) {
                    let n = name_at(bot, occ.0, occ.1, occ.2);
                    cast_debug(&format!("  scoop t={t:?}: ray occluded by {occ:?} ({n}) — digging it"));
                    // Never dig an occluder BELOW the feet or touching lava: that is the pool's rim /
                    // the stand's own floor (rust-gym-003 dug (399,-55,872) = its stand floor beside
                    // the lake, then died on the next fill). Treat it like obsidian: skip the source.
                    let below_feet = occ.1 < feet_y(bot);
                    let rim = [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0), (0, -1, 0)]
                        .iter()
                        .any(|&(ox, oy, oz)| is_lava(&name_at(bot, occ.0 + ox, occ.1 + oy, occ.2 + oz)));
                    if n == "obsidian" || below_feet || rim {
                        OBSIDIAN_BLOCKED.lock().unwrap().push(*t);
                    } else if !is_lava(&n) {
                        dig_at(bot, occ.0, occ.1, occ.2).await;
                        bot.wait_ticks(3).await.ok();
                    }
                    continue;
                }
                bot.activate_item().await.ok();
                bot.wait_ticks(10).await.ok();
                if count_items(bot, &filled_bucket_name) > before {
                    cast_debug(&format!("fill {fluid}: OK (round {round}) → {}", count_items(bot, &filled_bucket_name)));
                    return true;
                }
                // Server scooped it but the inventory didn't sync (the same raciness the
                // water fill predicts around): if the target source VANISHED right as we
                // used the bucket, the fill happened — mirror it locally so the step
                // machine sees the filled bucket instead of re-scooping an empty spot.
                let now = name_at(bot, t.0, t.1, t.2);
                if fluid == "lava" && was_lava && is_air(&now) && count_items(bot, "bucket") > 0 {
                    bot.ensure_item("lava_bucket", 1);
                    if let Some(s) = bot.inventory.slots.iter_mut().flatten().find(|i| i.name == "bucket") {
                        s.count -= 1;
                    }
                    cast_debug(&format!("fill lava: OK (round {round}, source vanished — predicted)"));
                    return true;
                }
                cast_debug(&format!("  scoop t={t:?} dy={dy:.1} held={held:?} lava {was_lava}->{now} pitch={sent_pitch:.0} yaw={sent_yaw:.0} reach={reach:.1}"));
            }
        }
    }
    cast_debug(&format!("fill {fluid}: all rounds failed"));
    false
}

// ── prepare site + build the whole portal ─────────────────────────────────────

/// Find a lava pool and clear a flat 6x6x5 casting chamber beside it; fill a lava
/// bucket from the pool (refilled each cast).
/// An exact lava-pool coord handed to the cast by the LavaPool gym (the bot was tp'd right next to
/// it). When set, `prepare_cast_site` uses it directly instead of a find_fluid/memory scan that
/// races empty right after the underground tp and blind-descends past the real pool. `None` in a
/// real run (the descent finds lava normally).
/// Lava pools retired as unscoopable this process. `mem.mark(.., Gone)` only updates an existing
/// POI row, so a pool the bot found by scanning (never recorded) stayed selectable: rust-gym-003
/// retired (489,-49,493) and the descent's lava steering tunnelled straight back to it, twice.
/// Every lava finder skips anything within 6 blocks of a retired pool.
/// Sources whose scoop ray is blocked by OBSIDIAN (undiggable — the mold's escaped water turns a
/// pool's surface cells to obsidian). The LOS check can't clear it, so without this the refill
/// retried the same blocked source forever: rust-gym-001 (natural, 7/10) spent ~15 min on
/// `ray occluded by (575,-55,1448) (obsidian)` and timed out. Skipped as scoop candidates.
static OBSIDIAN_BLOCKED: std::sync::Mutex<Vec<(i32, i32, i32)>> = std::sync::Mutex::new(Vec::new());

static RETIRED_LAVA: std::sync::Mutex<Vec<(i32, i32, i32)>> = std::sync::Mutex::new(Vec::new());
fn retire_lava(p: (i32, i32, i32)) {
    RETIRED_LAVA.lock().unwrap().push(p);
}
pub(crate) fn lava_retired(p: (i32, i32, i32)) -> bool {
    RETIRED_LAVA.lock().unwrap().iter().any(|r| (r.0 - p.0).pow(2) + (r.1 - p.1).pow(2) + (r.2 - p.2).pow(2) <= 144) // 12 blocks: one pool is one body (6 retired a deep pool one source at a time)
}

pub static FORCED_LAVA: std::sync::Mutex<Option<(i32, i32, i32)>> = std::sync::Mutex::new(None);
pub fn set_forced_lava(v: Option<(i32, i32, i32)>) {
    *FORCED_LAVA.lock().unwrap() = v;
}

/// The exact position from which the bot last scooped lava SUCCESSFULLY (and survived). Refills
/// during the frame cast navigate back HERE instead of letting fill_bucket re-find a stand from the
/// chamber side — which pillared beside the pool and burned the bot to death (`EARLY BAIL hp=0`).
pub(crate) static SAFE_SCOOP_STAND: std::sync::Mutex<Option<(f64, f64, f64)>> = std::sync::Mutex::new(None);

/// PERSISTENT frame anchor. Without it, each build_nether_portal call re-anchors the frame at the
/// bot's CURRENT position — so after a fire death (respawn) or a relocate for fresh lava, the bot
/// starts a NEW empty frame and abandons the partial one (013 built 4 obsidian, relocated, restarted
/// at 0/10). Stored on the first anchor, reused (walk back) on every later call, cleared once the
/// frame is complete, so obsidian accumulates in ONE frame across in-process respawns.
/// Obsidian among the 10 frame cells of the frame anchored at `(bx,by,bz)` (frame plane z = bz,
/// same cells as frame_check).
fn anchored_frame_present(bot: &Bot, (bx, by, bz): (i32, i32, i32)) -> usize {
    let mut cells = vec![(bx + 1, by), (bx + 2, by), (bx + 1, by + 4), (bx + 2, by + 4)];
    cells.extend((1..=3).flat_map(|dy| [(bx, by + dy), (bx + 3, by + dy)]));
    cells.iter().filter(|&&(x, y)| name_at(bot, x, y, bz) == "obsidian").count()
}

pub(crate) static FRAME_ANCHOR: std::sync::Mutex<Option<(i32, i32, i32)>> = std::sync::Mutex::new(None);
/// Re-site-after-death instrumentation (cycle 5): whether an anchor has been seen, the bot's death count
/// when it was first seen, and how many times a death was followed by a lost anchor.
static HAD_ANCHOR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static DEATHS_AT_ANCHOR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub(crate) static RESITE_AFTER_DEATH: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The anchor is also persisted to a per-bot FILE so a partial frame survives a PROCESS relaunch
/// (a stuck-bail → watchdog restart wipes the static, abandoning e.g. a 7/10 frame — but the
/// obsidian is still in the world, so a relaunched bot returns here and RESUMES instead of
/// rebuilding from scratch, which is what stopped completions across the churn).
fn frame_anchor_path() -> String {
    format!(".frame-{}.txt", std::env::var("MC_USERNAME").unwrap_or_else(|_| "bot".into()))
}
fn save_frame_anchor(a: (i32, i32, i32)) {
    let _ = std::fs::write(frame_anchor_path(), format!("{} {} {}", a.0, a.1, a.2));
}
fn load_frame_anchor() -> Option<(i32, i32, i32)> {
    let s = std::fs::read_to_string(frame_anchor_path()).ok()?;
    let n: Vec<i32> = s.split_whitespace().filter_map(|t| t.parse().ok()).collect();
    (n.len() == 3).then(|| (n[0], n[1], n[2]))
}
/// Move the persistent anchor (the mold shifts the frame off the lava pool's footprint).
pub(crate) fn set_frame_anchor(a: (i32, i32, i32)) {
    *FRAME_ANCHOR.lock().unwrap() = Some(a);
    save_frame_anchor(a);
}

pub(crate) fn clear_frame_anchor() {
    *FRAME_ANCHOR.lock().unwrap() = None;
    let _ = std::fs::remove_file(frame_anchor_path());
}

/// Obsidian in the 10 frame cells of the CURRENT persistent anchor (0 if no anchor / no chunk).
/// The main-loop stuck-guard folds this into its progress signature so a SLOWLY-advancing cast
/// (2/10 → 3/10 across step re-derives — the cast legitimately takes >1 step budget at ~2min/block
/// on natural terrain) registers as PROGRESS and never trips the 20-fail relaunch, which would
/// re-hunt lava and orphan the in-progress frame. Obsidian persists in-world across same-process
/// re-derives, so counting it is the honest progress metric the inventory sum can't see.
pub fn frame_obsidian_count(bot: &Bot) -> i32 {
    let Some((bx, by, bz)) = *FRAME_ANCHOR.lock().unwrap() else {
        return 0;
    };
    let at = |dx: i32, dy: i32| (bx + dx, by + dy, bz);
    let cells = [at(1, 0), at(2, 0), at(0, 1), at(0, 2), at(0, 3), at(3, 1), at(3, 2), at(3, 3), at(1, 4), at(2, 4)];
    cells.iter().filter(|(x, y, z)| name_at(bot, *x, *y, *z) == "obsidian").count() as i32
}

/// A SAFE respawn/tp stand near the frame anchor: solid non-lava floor at `by`, air for the body at
/// `by+1..by+2`. A FIXED lane offset (bz+2) was lethal on frames whose +Z side sits over the lava
/// sea — the tp landed the bot on AIR and it fell to the y-55 sea (died twice at 7/10 at the very
/// tp cell). Scan the working area (lane first, then -Z) and pick a VERIFIED cell; the bottom-row
/// obsidian itself qualifies once cast. Fall back to the lane only if nothing qualifies.
fn safe_stand_near_frame(bot: &Bot, bx: i32, by: i32, bz: i32) -> (i32, i32, i32) {
    // Scan SEVERAL feet heights, HIGHEST first (further from the sea): a frame whose whole y=by ring
    // is over lava had no qualifying cell, so the fixed-offset fallback fired and the bot tp'd into
    // the sea at that exact cell repeatedly (rust-race-002, died 2x at (909.5,-53,466.5) = the
    // fallback). The frame's own scaffold top / cast obsidian (solid, never lava) is a guaranteed
    // safe stand once the bottom row exists — this finds it.
    // Mold: only the WORKING LANE rows (z=bz+1, bz+2: pad floor at by-1, platform blocks the mold
    // placed itself — the local view of those is reliable). The old scan also accepted the frame
    // row and rows NORTH of it (the sea side) and trusted the local world for cells under the lava
    // surface: rust-gym-003 (natural) was tp'd to the same cell two blocks under the lava seven
    // times in a row — a death loop that ate the whole 2400 s trial. Highest stand first.
    let lava_ring = |x: i32, feet: i32, z: i32| {
        (-1..=1).any(|dx| (-1..=1).any(|dz| is_lava(&name_at(bot, x + dx, feet - 1, z + dz)) || is_lava(&name_at(bot, x + dx, feet, z + dz))))
    };
    for feet in (by..=by + 6).rev() {
        for dz in [2, 1] {
            for dx in [2, 1, 3, 0, 4, -1] {
                let (x, z) = (bx + dx, bz + dz);
                let floor = name_at(bot, x, feet - 1, z);
                if is_solid(&floor) && !is_lava(&floor) && is_air(&name_at(bot, x, feet, z)) && is_air(&name_at(bot, x, feet + 1, z)) && !lava_ring(x, feet, z) {
                    return (x, feet, z);
                }
            }
        }
    }
    // The pad floor (by-1) under the back row is always solid once the mold has started.
    (bx + 2, by, bz + 2)
}

/// Respawn, then TP back to the frame. A death otherwise dumps the bot at WORLD SPAWN (surface):
/// the frame's `spawnpoint` at a deep, lava-adjacent cell is rejected by the server as an unsafe
/// respawn point, so every death forces a ~100-block RE-DESCENT to the band — the tax that caps the
/// frame at a few blocks (climb 2-3, die, re-descend ~5min, climb 2-3, …). The bot is op'd, so a
/// direct `tp @s` to the frame stand makes a death cost seconds, not minutes, letting the frame
/// actually grind to 10/10. No-op tp if there's no anchor yet (pre-frame deaths still respawn).

/// Deaths handled inside the cast (respawn_at_frame) — drained by the app loop into race.db.
static CAST_DEATHS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub fn take_cast_deaths() -> u32 {
    CAST_DEATHS.swap(0, std::sync::atomic::Ordering::Relaxed)
}

pub(crate) async fn respawn_at_frame(bot: &mut Bot<'_>) {
    CAST_DEATHS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    bot.respawn().await.ok();
    bot.wait_ticks(5).await.ok();
    let anchor = *FRAME_ANCHOR.lock().unwrap(); // copy the Option, drop the guard before awaiting
    if let Some((bx, by, bz)) = anchor {
        // Land on the WORKING LANE (+Z of the frame, cobble-floored by the cast), NOT the anchor
        // cell: the anchor is the frame's bottom-left cup, which after casting holds lava / sits at
        // the sea edge — tp-ing onto it dropped the bot straight into fire (live: died twice at the
        // same (898.5,-53.9,585.5) right after each tp-back → a death loop at 4/10).
        let (sx, sy, sz) = safe_stand_near_frame(bot, bx, by, bz);
        let _ = bot.run_command(&format!("tp @s {sx} {sy} {sz}")).await;
        bot.wait_ticks(10).await.ok();
        let p = bot.entity.position;
        cast_debug(&format!(
            "respawn → tp ({sx},{sy},{sz}) floor={} now=({:.1},{:.1},{:.1}) hp={:.0}",
            name_at(bot, sx, sy - 1, sz), p.x, p.y, p.z, bot.health
        ));
    }
}

/// One tunnel step at the lava band: the pathfinder goto (`strip_tunnel`) first; if the bot didn't
/// move, dig the 1×2 cells ahead directly and walk in. The pathfinder refuses to break blocks next
/// to fluids, so near a lake it can return NoPath instantly: rust-gym-002 logged `tunnelling toward
/// (-1,0)` ~430× at hd 12–13 without moving (00:27 binary, 2400 s timeout). Never digs a cell that
/// has lava beside/above/below it (that's the pool's edge — the scoop takes over from there).
async fn tunnel_step(bot: &mut Bot<'_>, dx: i32, dz: i32) {
    // DIRECT step first — dig the 1×2 cells ahead (never a lava-touching cell, never over a
    // missing floor) and walk in. The pathfinder goto (strip_tunnel, 6 ahead) is only the fallback
    // for a blocked direct step: it timed out on nearly every step (~20 s each even along an open
    // cave corridor — rust-gym-003: one block per ~21 s at (1179,-54,1126..1128)).
    let p0 = bot.entity.position;
    let (fx, fz, fy) = (p0.x.floor() as i32, p0.z.floor() as i32, feet_y(bot));
    let (ax, az) = (fx + dx, fz + dz);
    let lava_touch = |x: i32, y: i32, z: i32| {
        [(0, 0, 0), (1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0), (0, -1, 0)]
            .iter()
            .any(|&(ox, oy, oz)| is_lava(&name_at(bot, x + ox, y + oy, z + oz)))
    };
    // Lava touching the next cell means the tunnel has REACHED the pool: cap what touches it and stay
    // put, so the caller's at-depth check hands over to the scoop (the station). The pathfinder detour
    // (strip_tunnel) through lava-adjacent rock killed rust-gym-001 in batch 3 (`direct step blocked …
    // — pathfinder`, dead 7 s later at the −55 lake). Only a missing floor (no lava) still detours.
    if lava_touch(ax, fy, az) || lava_touch(ax, fy + 1, az) {
        // Cap EVERY lava cell lava_touch looked at (the next cell's column and its sides, at feet and
        // head height). Capping only the next cell's own column left side lava in place (`capped 0`).
        let mut capped = 0;
        let mut cells: Vec<(i32, i32, i32)> = Vec::new();
        for y in [fy, fy + 1] {
            for (ox, oy, oz) in [(0, 0, 0), (1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0), (0, -1, 0)] {
                let c = (ax + ox, y + oy, az + oz);
                if (c.0, c.2) != (fx, fz) && is_lava(&name_at(bot, c.0, c.1, c.2)) && !cells.contains(&c) {
                    cells.push(c);
                }
            }
        }
        for c in cells {
            if place_cobble(bot, c).await {
                capped += 1;
            }
        }
        // Staying put is only safe if our OWN cell is lava-free. Batch 6 rust-gym-002 stayed put three
        // times (`capped 0`) with lava beside its body and died. Lava still touching the feet or head
        // cell → step back one cell, opposite the direction of travel, and let the caller re-plan.
        let own_hot = [fy, fy + 1].iter().any(|&y| {
            [(1, 0), (-1, 0), (0, 1), (0, -1), (0, 0)].iter().any(|&(ox, oz)| is_lava(&name_at(bot, fx + ox, y, fz + oz)))
        }) || is_lava(&name_at(bot, fx, fy - 1, fz));
        if own_hot {
            cast_debug(&format!("tunnel_step: lava touches our own cell at ({fx},{fy},{fz}) after capping {capped} — stepping back"));
            walk_to_xz(bot, (fx - dx) as f64 + 0.5, (fz - dz) as f64 + 0.5, 0.3, 20).await;
            return;
        }
        cast_debug(&format!("tunnel_step: lava at the next cell ({ax},{fy},{az}) dir=({dx},{dz}) — capped {capped}, staying put (no pathfinder detour)"));
        return;
    }
    if !solid_at(bot, ax, fy - 1, az) {
        cast_debug(&format!("tunnel_step: no floor at ({ax},{},{az}) dir=({dx},{dz}) — pathfinder", fy - 1));
        strip_tunnel(bot, dx, dz).await;
        return;
    }
    for c in [(ax, fy + 1, az), (ax, fy, az)] {
        if !is_air(&name_at(bot, c.0, c.1, c.2)) {
            dig_at(bot, c.0, c.1, c.2).await;
        }
    }
    // Let a rejected break come back from the server before stepping in (the SDK predicts air).
    bot.wait_ticks(5).await.ok();
    if !is_air(&name_at(bot, ax, fy, az)) || !is_air(&name_at(bot, ax, fy + 1, az)) {
        cast_debug(&format!("tunnel_step: ({ax},{fy},{az}) still solid after the dig — pathfinder"));
        strip_tunnel(bot, dx, dz).await;
        return;
    }
    walk_to_xz(bot, ax as f64 + 0.5, az as f64 + 0.5, 0.3, 30).await;
}

/// A WATER BODY ahead of the shaft: water directly below within 3, or ≥ 8 water blocks in the
/// 5×3×5 box below the feet (an aquifer or lake beside the column floods the shaft through a thin
/// wall). A small pocket beside the column is not a hazard. Counting any single neighbour made every
/// step past a ~6-block pocket a failed detour, and the fall-through to the fluid-cautious dig_down
/// crawled ~25 s/block (batch 2 rust-gym-005: y 64 → 51 in 330 s).
fn wet_column(bot: &Bot, x: i32, feet: i32, z: i32) -> bool {
    if (1..=3).any(|d| name_at(bot, x, feet - d, z).contains("water")) {
        return true;
    }
    // LAVA anywhere in the 3×3 below within 3: never dig toward it on the way down. `!is_lava(below)`
    // only saw the block directly under the feet, so the shaft dropped into a near-surface lava lake
    // 2–3 below. Respawning at the shaft top, the bot re-dug the same shaft and died 31× in 7 min
    // (batch 2 rust-gym-003, trial 2, lake at y 54 under the spawn).
    if (1..=3).any(|d| (-1..=1).any(|ox| (-1..=1).any(|oz| is_lava(&name_at(bot, x + ox, feet - d, z + oz))))) {
        return true;
    }
    let mut n = 0;
    for dy in 1..=3 {
        for ox in -2..=2 {
            for oz in -2..=2 {
                if name_at(bot, x + ox, feet - dy, z + oz).contains("water") {
                    n += 1;
                }
            }
        }
    }
    n >= 8
}

/// Move the descent shaft sideways through DRY rock: try the four directions (starting from `seed`),
/// up to 6 direct 1×2 tunnel steps each, never opening a cell that touches water or lava. True once
/// the bot has moved ≥ 2 blocks to a column with no water within 3 below its feet.
async fn detour_dry(bot: &mut Bot<'_>, seed: u32) -> bool {
    let fluid = |n: &str| n.contains("water") || is_lava(n);
    for k in 0..4u32 {
        let (dx, dz) = [(1, 0), (0, 1), (-1, 0), (0, -1)][((seed + k) % 4) as usize];
        let p0 = bot.entity.position;
        let mut why = "6 steps";
        for _ in 0..6 {
            let p = bot.entity.position;
            let (fx, fy, fz) = (p.x.floor() as i32, feet_y(bot), p.z.floor() as i32);
            let (ax, az) = (fx + dx, fz + dz);
            let touches = |x: i32, y: i32, z: i32| {
                [(0, 0, 0), (1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, 1, 0), (0, -1, 0)]
                    .iter()
                    .any(|&(ox, oy, oz)| fluid(&name_at(bot, x + ox, y + oy, z + oz)))
            };
            if touches(ax, fy, az) || touches(ax, fy + 1, az) {
                why = "fluid at the next cell";
                break;
            }
            if !solid_at(bot, ax, fy - 1, az) {
                why = "no floor ahead";
                break;
            }
            tunnel_step(bot, dx, dz).await;
            if bot.entity.position.x.floor() as i32 == fx && bot.entity.position.z.floor() as i32 == fz {
                why = "didn't move";
                break;
            }
        }
        let p = bot.entity.position;
        let moved = ((p.x - p0.x).powi(2) + (p.z - p0.z).powi(2)).sqrt();
        let (px, py, pz) = (p.x.floor() as i32, feet_y(bot), p.z.floor() as i32);
        let dry_below = !wet_column(bot, px, py, pz);
        cast_debug(&format!("  detour dir=({dx},{dz}): moved {moved:.1}, stop={why}, dry_below={dry_below}"));
        if moved >= 2.0 && dry_below {
            return true;
        }
    }
    false
}

async fn prepare_cast_site(bot: &mut Bot<'_>, mem: &mut WorldMemory) -> Option<(i32, i32, i32)> {
    let deadline = Instant::now() + Duration::from_secs(600);

    // Let any pending block updates settle so the bot's local world is current
    // before we scan for lava (an RCON-placed / freshly-revealed pool may not be in
    // the world yet on the very first tick).
    bot.wait_ticks(10).await.ok();
    // 1. Locate visible lava; if none, dig down toward cave-lava depth and retry.
    // Keep the radius modest — a 30-block exposed scan is ~226k synchronous block
    // lookups that block the network loop past the keep-alive timeout (→ kick).
    // Only accept lava with a SCOOPABLE source in reach — a flowing-only pocket looks like
    // lava to find_fluid but scoops nothing, and settling on it skips the descent that
    // would reach a real source lake.
    let mut lava = find_fluid(bot, "lava", 16).filter(|_| source_lava_near(bot, 12));
    {
        let p = bot.entity.position;
        cast_debug(&format!("prepare: at ({:.0},{:.0},{:.0}) lava={lava:?}", p.x, p.y, p.z));
    }
    // LavaPool gym: the finder tp'd the bot right next to a KNOWN-EXPOSED pool and handed us its
    // coord. Use it directly — navigate there, wait for the post-tp chunks to settle, and TRUST it
    // even if find_fluid still can't confirm this tick (the finder already verified exposed lava).
    // This beats the memory/observe path that chased a nearer "dry" POI and blind-descended past
    // the real pool to un-scoopable deep seas.
    if let Some(forced) = *FORCED_LAVA.lock().unwrap() {
        bot.movement.blocks_cant_break.clear();
        let _ = bot.goto_near(forced.0, forced.1 + 1, forced.2, 3.0).await;
        // If we couldn't get anywhere NEAR the forced pool, it's stale/unreachable — e.g. the gym
        // tp'd the bot to a deep pool but it died/rubber-banded back to spawn, leaving FORCED_LAVA
        // pointing thousands of blocks away. Trusting it then loops forever ("approached lava, now
        // 4400 away → retiring pool") without ever descending. Discard it and fall through to the
        // local memory / mining-descent path so the bot finds real lava wherever it actually is.
        let p = bot.entity.position;
        let dist_to_forced =
            ((forced.0 as f64 - p.x).powi(2) + (forced.2 as f64 - p.z).powi(2)).sqrt();
        if dist_to_forced > 48.0 {
            cast_debug(&format!("prepare: FORCED_LAVA {forced:?} is {dist_to_forced:.0} away (unreachable — likely a failed tp/death) — discarding, will descend to local lava"));
            *FORCED_LAVA.lock().unwrap() = None;
        } else {
            for _ in 0..10 {
                lava = find_fluid(bot, "lava", 20).filter(|_| source_lava_near(bot, 16));
                if lava.is_some() {
                    break;
                }
                bot.wait_ticks(20).await.ok();
            }
            if lava.is_none() {
                lava = Some(forced);
            }
            cast_debug(&format!("prepare: FORCED_LAVA {forced:?} → lava={lava:?}"));
        }
    }
    if lava.is_none() {
        // First, reuse any lava the bot remembered while iron-mining (it was AT lava
        // depth then, and find_exposed_blocks logged what it passed). Navigate to the
        // nearest remembered pool and rescan — this is the reliable path: the bot
        // already found lava once, just go back to it.
        let p = bot.entity.position;
        let from = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if let Some(poi) = mem.nearest(&[PoiKind::Lava], from, 4) {
            cast_debug(&format!("prepare: heading to remembered lava {:?}", poi.pos));
            bot.movement.blocks_cant_break.clear();
            let _ = bot.goto_near(poi.pos.0, poi.pos.1, poi.pos.2, 3.0).await;
            // Chunks after an underground tp (the LavaPool gym) can take a second to settle, so
            // find_fluid/source_lava_near race empty at a pool that's really there — which marked
            // the pool "dry" and blind-descended right past it. Retry the scan a few times with
            // waits before giving up on a remembered pool.
            for _ in 0..6 {
                lava = find_fluid(bot, "lava", 16).filter(|_| source_lava_near(bot, 12));
                if lava.is_some() {
                    break;
                }
                bot.wait_ticks(20).await.ok();
            }
            // Dry on arrival — a single exposed block since covered, or no scoopable source
            // at the recorded spot. Retire it (Gone) so the next attempt doesn't re-chase the
            // SAME dead coord: without this the bot loops navigating to one stale POI forever
            // (observed 4x on 888,-16,400) instead of falling through to the deep-descent that
            // reliably reaches a real lake. `observe` preserves Gone, so it won't resurrect.
            if lava.is_none() {
                cast_debug(&format!("prepare: remembered lava {:?} dry on arrival → Gone", poi.pos));
                mem.mark(poi.pos, PoiStatus::Gone);
            }
        }
    }
    if lava.is_none() {
        // No remembered lava in reach. Reach the lava band by digging a 1×1 shaft STRAIGHT
        // DOWN (accepting the risk of dropping into lava — see the descent body below), then
        // tunnelling ACROSS at depth until an exposed source comes into reach with a solid
        // tunnel floor to stand on and scoop from.
        bot.movement.blocks_cant_break.clear();
        let mut desc_relocate = 0u32;
        let mut wet_detour_fails = 0u32;
        // Reset the relocate escalation only on a NEW lowest depth. Bobbing in an aquifer (feet
        // -35 → -36 → -35 …) counted every 1-block dip as progress and pinned the escape at
        // relocate#1 (12 blocks) forever: rust-gym-002 logged it 96× and never left the water.
        let mut desc_min_y = i32::MAX;
        // LAVA SITING by chunk scan (cycle 2 Phase C): the nearest cluster of ≥ 40 lava sources
        // within 12 blocks, below the tunnel floor, from ALL loaded chunk data (not just exposed
        // lava). Rescanned at most every 10 s as new chunks load.
        let mut cluster: Option<((i32, i32, i32), usize)> = None;
        let mut cluster_scanned: Option<Instant> = None;
        // Tunnel progress per target: (target, best horizontal distance, when it last improved).
        let mut tun_best: Option<((i32, i32, i32), f64, Instant)> = None;
        let site_t0 = Instant::now();
        for _ in 0..240 {
            if Instant::now() > deadline {
                break;
            }
            // Dead mid-descent: the client keeps "descending" as a ghost. rust-gym-002 (12:11 binary)
            // drowned at 276 s (server: `drowned`) and logged 3 min of descent + tunnelling 37 blocks
            // below its body before `pre-scoop heal → hp=0` (respawn tp: `moved too quickly 0,-37.8,0`).
            if bot.health <= 0.0 {
                cast_debug("desc: DEAD mid-descent — respawning");
                respawn_at_frame(bot).await;
                return None;
            }
            // Reached a SCOOPABLE lava lake at DEPTH? Stop and scoop. Gated on feet<=-50 so we
            // don't scoop a shallow sea we DROP past on the way down — dropping in from above
            // leaves the bot IN the sea with no rim (unscoopable). Instead we keep digging down
            // to the lava band (~y-54) and reach lava by TUNNELLING across into it, where the
            // tunnel floor is a solid stone rim to stand on. A flowing-only pocket is skipped.
            // -50 also forced the bot PAST shallow/elevated source lava (dominant live failure: a
            // y-40 pool the shaft dug past to -54, ending 14 blocks UNDER it → 0 scoopable
            // sources, 109×). Stop for elevated lava too WHEN standing on solid ground (a real
            // rim, not mid-fall into a sea); the deep band still qualifies via feet<=-50. A bad
            // stop scoops nothing and falls through to fill_bucket + the relocate-on-fail path.
            let scoop_fy = feet_y(bot);
            let (scoop_fx, scoop_fz) =
                (bot.entity.position.x.floor() as i32, bot.entity.position.z.floor() as i32);
            let on_solid = solid_at(bot, scoop_fx, scoop_fy - 1, scoop_fz);
            // Settle only on a pool find_fluid will actually return — it skips RETIRED pools, and
            // breaking on a retired one returned None at once and looped (rust-gym-003: 782 ×
            // `prepare: at (303,-54,669) lava=None` in one trial after the climb-out was removed).
            if (scoop_fy <= -50 || (scoop_fy <= -35 && on_solid)) && source_lava_near(bot, 5) {
                if let Some(l) = find_fluid(bot, "lava", 24) {
                    lava = Some(l);
                    break;
                }
            }
            // DIG STRAIGHT DOWN (1×1) to the lava band, then tunnel ACROSS. Per the operator:
            // accept the risk of dropping into lava — the gym's goal is perfecting the CAST logic
            // once at depth, so reach the lava layer ASAP with no safe-descent relocate/cavern
            // dance. The portal phase holds buckets/flint, so keep a pickaxe equipped or hard
            // stone (deepslate/andesite) can't be broken and the shaft stalls.
            ensure_pickaxe(bot).await;
            let fy = feet_y(bot);
            let (px, pz) = (bot.entity.position.x.floor() as i32, bot.entity.position.z.floor() as i32);
            let below = name_at(bot, px, fy - 1, pz);
            // Never sink into DEEP water: no buoyancy, slow underwater digs, and the bot drowns
            // on the way down (server: `rust-gym-001 drowned` 19:16 UTC after sinking y 47 → <38
            // through a flooded column; the shaft seal can't help in open water). Water ≥3 deep
            // below → relocate sideways off the water body instead.
            // LOOK AHEAD: water within 3 below the feet means the next digs open into it. Detour sideways
            // through rock BEFORE breaking in. Checking only the block below (the old rule) fired once the
            // bot already stood over the water at the bottom of a 1×1 shaft, where a sideways goto can't
            // move, so it "sank through instead" and died under sealed water (batch 1, rust-gym-001 at
            // y 30, twice). Already wet → escape first; the detour starts from the dry shaft.
            // Above the −50 floor only: at the floor, lava below IS the target (reached by tunnelling).
            // HEAD UNDER WATER → escape before any dig. `wet_column` only looks BELOW the feet; a drop
            // into a cave flooded from above leaves the water over the head, so the loop kept digging
            // deepslate underwater (`water over the head … sealed above → false` ×20) and the bot
            // drowned (batch 2, rust-gym-005, 05:14 UTC, y −24..−26, ~100 s submerged).
            if crate::bot_utils::head_in_water(bot) {
                cast_debug(&format!("desc: head under water at y={fy} — escaping before digging"));
                crate::bot_utils::leave_water(bot, 200).await;
                desc_relocate += 1;
                continue;
            }
            let wet_ahead = fy > -50 && wet_column(bot, px, fy, pz);
            if wet_ahead {
                if crate::bot_utils::feet_in_water(bot) || crate::bot_utils::head_in_water(bot) {
                    crate::bot_utils::leave_water(bot, 200).await;
                }
                if detour_dry(bot, desc_relocate).await {
                    desc_relocate += 1;
                    cast_debug(&format!("desc: water within 3 below y={fy} — detoured sideways through rock (#{desc_relocate})"));
                    continue;
                }
                desc_relocate += 1;
                cast_debug(&format!("desc: water within 3 below y={fy} — no dry detour (#{desc_relocate})"));
                wet_detour_fails += 1;
                // Don't dig down toward the water after a failed detour (water_descent: y 75 → 74 between
                // failed tries); retry, rotating the direction. Only after 4 misses fall through to the old
                // deep-water rule below as a last resort.
                if wet_detour_fails < 4 {
                    continue;
                }
            }
            if below.contains("water") {
                let depth = (1..=12).take_while(|d| name_at(bot, px, fy - d, pz).contains("water")).count();
                if depth >= 3 {
                    desc_relocate += 1;
                    let (dx, dz) = [(0, 1), (1, 0), (0, -1), (-1, 0)][(desc_relocate as usize) % 4];
                    let dist = 8 + (desc_relocate.min(8) as i32) * 4;
                    cast_debug(&format!("desc: water {depth}+ deep below y={fy} — relocate#{desc_relocate} ({dx},{dz})x{dist}"));
                    let p0 = bot.entity.position;
                    let _ = bot.goto_xz(px + dx * dist, pz + dz * dist, 2.0).await;
                    let p1 = bot.entity.position;
                    // Already IN the water body (the bot can't swim, so the goto goes nowhere):
                    // retrying just bobs in place — rust-gym-003 (13:58 binary) sat at (918.3,740.7)
                    // through relocate#1..#5. Fall through to the old sink/punch-through path.
                    if ((p1.x - p0.x).powi(2) + (p1.z - p0.z).powi(2)).sqrt() >= 2.0 {
                        continue;
                    }
                    cast_debug("desc: relocate went nowhere (in the water) — sinking through instead");
                }
            }
            // Descend to −50, ABOVE the lava layer (lakes sit at −55…−60), not into it at −54: the
            // chunk-scan siting below finds lava under the floor, and cycle-1 deaths were drops into it.
            // SEAL THE SHAFT: before digging the block under the feet (the body's next cell), plug any
            // water or lava beside it with cobble, using that block's own side face as support. Past an
            // aquifer the 1×1 shaft flooded from the side, and by the time the look-ahead fired every
            // detour direction touched water (batch 4 detour log: `stop=fluid at the next cell, moved
            // 0.0` ×4, every time). A sealed shaft stays dry, so the detour can leave through rock.
            if fy > -50 {
                // fy−1 is the next cell's sides (water/lava). fy and fy+1 are the body's own sides, for
                // LAVA only: a drop through caves can land the bot beside a lava flow at body height, and
                // the 3×3-below look-ahead never sees it (regression 4, rust-gym-007: fell −22 → −38
                // through air, no dry detour, one dig at −40, dead in lava).
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    for dy in [-1, 0, 1] {
                        let c = (px + dx, fy + dy, pz + dz);
                        let n = name_at(bot, c.0, c.1, c.2);
                        if (dy == -1 && n.contains("water")) || is_lava(&n) {
                            if !place_cobble(bot, c).await {
                                cast_debug(&format!("desc: shaft seal at {c:?} ({n}) failed"));
                            }
                        }
                    }
                }
            }
            if fy > -50 && !is_lava(&below) {
                // FAST path: the miner's dig_down centres the bot, digs with proper break timing,
                // and drops in a single pass. A hand-rolled dig+wait under-waited and re-dug the
                // same block for many passes (dug=1 but y unchanged), crawling the descent to ~14
                // min. dig_down refuses over a ledge/cave AND — near the band — refuses plain stone
                // when its long-range avoidance senses buried lava; the corner fallback covers both.
                let mut dug = 0;
                // CENTRE in the shaft cell first: dig_down digs every cell the 0.6-wide box
                // straddles (up to 4), each with its own settle + look + dig + wait — ~4 s per
                // block (rust-gym-002: 1191 s of a 2400 s trial spent on 235 descent steps). A
                // centred bot digs ONE cell and falls straight, staying centred in the 1×1 shaft.
                {
                    let p = bot.entity.position;
                    let (cx, cz) = (px as f64 + 0.5, pz as f64 + 0.5);
                    if (p.x - cx).abs() > 0.2 || (p.z - cz).abs() > 0.2 {
                        walk_to_xz(bot, cx, cz, 0.15, 12).await;
                    }
                }
                let dropped_fast = dig_down(bot).await;
                if !dropped_fast {
                    // Centre on the cell so there's a single support column (a between-cells bot
                    // re-settled onto an un-dug corner and never fell), then dig every footprint
                    // corner — a ledge perch holds the bot at a CORNER, not the centre. Skip a
                    // cell with lava directly below; otherwise accept the operator's death risk.
                    walk_to_xz(bot, px as f64 + 0.5, pz as f64 + 0.5, 0.2, 12).await;
                    let p = bot.entity.position;
                    let mut cells: Vec<(i32, i32)> = Vec::new();
                    for ddx in [-0.3, 0.3] {
                        for ddz in [-0.3, 0.3] {
                            let c = ((p.x + ddx).floor() as i32, (p.z + ddz).floor() as i32);
                            if !cells.contains(&c) {
                                cells.push(c);
                            }
                        }
                    }
                    for (cx, cz) in cells {
                        if is_lava(&name_at(bot, cx, fy - 1, cz)) {
                            continue;
                        }
                        if is_solid(&name_at(bot, cx, fy - 1, cz)) {
                            dig_at(bot, cx, fy - 1, cz).await;
                            dug += 1;
                        }
                    }
                    bot.wait_ticks(6).await.ok();
                }
                cast_debug(&format!("desc y={fy}->{} below={below} dug={dug} fast={dropped_fast} at ({px},{pz})", feet_y(bot)));
                // Water flooding the shaft from above keeps the head submerged — the bot has no
                // buoyancy to surface, and underwater digs are ~5× slower. Two drownings in 15 min
                // (server: `rust-gym-002 drowned` 18:16 UTC, `rust-gym-001 drowned` 18:26 UTC), both
                // mid-descent below water. Seal the shaft above the head so the flow drains.
                {
                    let (hx, hz, hy) = (bot.entity.position.x.floor() as i32, bot.entity.position.z.floor() as i32, feet_y(bot));
                    if name_at(bot, hx, hy + 1, hz).contains("water") || name_at(bot, hx, hy + 2, hz).contains("water") {
                        let ok = place_cobble(bot, (hx, hy + 2, hz)).await;
                        cast_debug(&format!("desc: water over the head at ({hx},{},{hz}) — sealed above → {ok}", hy + 1));
                    }
                }
                // Didn't drop this pass — the fast corner-dig only handles a SOLID floor. Perched
                // on a ledge / over a cave (below=air, dug=0), on a slope, or over a water pocket
                // it stalls forever ("y=57->57 below=air dug=0" x900). Fall back to the proven
                // primitives: step DOWN a slope (descend_step), punch through a water aquifer, and
                // if still stuck relocate a few blocks to fresh diggable ground (walking off the
                // ledge into the gap so the bot falls). Any real drop resets the relocate counter.
                if feet_y(bot) >= fy {
                    let mut moved = false;
                    for &(dx, dz) in &[(0, 1), (1, 0), (0, -1), (-1, 0)] {
                        if descend_step(bot, dx, dz).await {
                            moved = true;
                            break;
                        }
                    }
                    if !moved && !raw_lava_near(bot, 3) {
                        moved = punch_through_water(bot).await;
                    }
                    if !moved {
                        desc_relocate += 1;
                        let (dx, dz) = [(0, 1), (1, 0), (0, -1), (-1, 0)][(desc_relocate as usize) % 4];
                        let p = bot.entity.position;
                        // Stuck in a WATER aquifer (below=water, nothing solid to dig and the bot
                        // won't sink) → the small relocate stays INSIDE the aquifer and it spins
                        // forever (013 stuck y49-53 relocate#1..3 endlessly). Jump FAR to escape
                        // the water body and find dry ground with a clear descent to the lava band.
                        let dist = if below.contains("water") {
                            8 + (desc_relocate.min(8) as i32) * 4
                        } else {
                            2 + (desc_relocate.min(6) as i32)
                        };
                        cast_debug(&format!("desc STUCK y={fy} below={below} → relocate#{desc_relocate} ({dx},{dz})x{dist}"));
                        let _ = bot.goto_xz(p.x.floor() as i32 + dx * dist, p.z.floor() as i32 + dz * dist, 2.0).await;
                    } else if feet_y(bot) < desc_min_y {
                        desc_min_y = feet_y(bot);
                        desc_relocate = 0;
                    }
                } else if feet_y(bot) < desc_min_y {
                    desc_min_y = feet_y(bot);
                    desc_relocate = 0;
                }
            } else {
                // At the lava band. Bore a beeline TOWARD any detected lava so the bot's own
                // tunnel reaches the pool edge — the tunnel floor (solid stone at the lava's own
                // level) is the rim fill_bucket stands on to scoop. strip_tunnel avoids stepping
                // INTO lava, so it stops flush at the edge, adjacent + in reach. Only if nothing
                // is detected yet does it tunnel a fixed direction to expose new ground.
                // Prefer lava at/near the bot's OWN level (scoopable from one-above) over an
                // ELEVATED pool it can't climb to. From the -54 band the bot fixated on a y-38
                // pool 6-7 blocks up/away (out of reach forever, "lava true->lava"). Search
                // band-level lava (|y-feet|<=3, exposed) first; only fall back to any lava.
                let scoop_feet = feet_y(bot);
                let bp = bot.entity.position;
                let band_lava = bot
                    .find_exposed_blocks("lava", 24, 64)
                    .into_iter()
                    .filter(|&(x, y, z)| y < scoop_feet && y >= scoop_feet - 3 && is_air(&name_at(bot, x, y + 1, z)) && !lava_retired((x, y, z)))
                    .min_by_key(|&(x, _, z)| {
                        (((x as f64 - bp.x).powi(2) + (z as f64 - bp.z).powi(2)) * 100.0) as i64
                    });
                // Beyond the 24-block scans: the nearest EXPOSED band-level lava anywhere in the
                // loaded chunks (±96) steers the tunnel — two natural bots bored 75+ blocks of
                // blind +Z at −54 with lava lakes elsewhere in view distance the whole time.
                let far_band_lava = || {
                    bot.find_loaded_nearest(&["lava"], (scoop_feet - 3, scoop_feet - 1))
                        .filter(|&(x, y, z)| is_air(&name_at(bot, x, y + 1, z)) && !lava_retired((x, y, z)))
                };
                // Only lava BELOW the tunnel floor steers the tunnel: pools above it cannot be approached from
                // the tunnel (the pathfinder won't climb to a rim beside lava) and were retired after the
                // walk — 24 retirements tonight, and one bot wore out all three pickaxes doing it.
                let below_floor = |l: &(i32, i32, i32)| l.1 < scoop_feet;
                if band_lava.is_none() && cluster_scanned.is_none_or(|t| t.elapsed() > Duration::from_secs(10)) {
                    cluster_scanned = Some(Instant::now());
                    // Lower bound −58: sources in the bedrock band (−64..−59) sit under/among bedrock and
                    // cannot be opened (race i6: a 113-source cluster at −62 held rust-race-003 for 3 h).
                    let found = bot.find_lava_cluster((-58, scoop_feet - 1), 100, 12, &|s| lava_retired(s)); // ≥100: big lakes (hour-15 "reduce ambition")
                    if found.is_some() && found.map(|f| f.0) != cluster.map(|c| c.0) {
                        let (c, n) = found.unwrap();
                        let p = bot.entity.position;
                        let d = ((c.0 as f64 - p.x).powi(2) + (c.2 as f64 - p.z).powi(2)).sqrt();
                        cast_debug(&format!("SITE cluster {n} sources near {c:?}, {d:.0} blocks away, {:.0}s into the at-depth search", site_t0.elapsed().as_secs_f64()));
                        let st = crate::state::sync_from_bot(bot);
                        mem.race_event("site", "cluster", Some("build_nether_portal"), &format!("{c:?} d={d:.0} t={:.0}s", site_t0.elapsed().as_secs_f64()), &st, n as i64);
                    }
                    cluster = found;
                }
                let cluster_target = cluster.map(|c| c.0).filter(|l| !lava_retired(*l));
                if let Some(l) = band_lava.or(cluster_target).or_else(|| find_fluid(bot, "lava", 24).filter(below_floor)).or_else(far_band_lava) {
                    let p = bot.entity.position;
                    let (ddx, ddz) = (l.0 as f64 - p.x, l.2 as f64 - p.z);
                    let hd = (ddx * ddx + ddz * ddz).sqrt();
                    // Once HORIZONTALLY close, stop tunnelling and hand off to the 3D approach
                    // (goto_near digs + CLIMBS). The lava is often ELEVATED and offset (sources at
                    // y-41..-48 while the shaft bottomed at -54): tunnelling stays flat at -54, 6-7
                    // blocks UNDER the pool, so source_lava_near(5) never fires and the bot bores
                    // sideways forever. Breaking on horizontal proximity lets the approach rise to it.
                    // Break early ONLY for ELEVATED lava (above the bot) — the 3D approach rises to
                    // it. For BAND-LEVEL lava (at/below the bot's y), keep tunnelling to FLUSH-
                    // ADJACENT (hd<=1.5): stopping at 3.5 left the bot ~3.6 away, and from there the
                    // fluid raytrace is too long / terrain-obstructed → "lava true->lava" even at
                    // reach 3. Adjacent gives the clean downward scoop that fills the bucket.
                    let elevated = l.1 > feet_y(bot);
                    // 2.6, not 1.6: the last tunnel cell often touches lava, so `tunnel_step` refuses it
                    // and the bot sat at hd=2 "tunnelling toward" until the 40-min timeout (batch 10
                    // rust-gym-004). 2.6 is well inside bucket reach; 3.5 was the distance that missed.
                    if (elevated && hd <= 3.5) || (!elevated && hd <= 2.6) {
                        cast_debug(&format!("desc at-depth y={fy} lava@{l:?} hd={hd:.0} elev={elevated} — handing to scoop"));
                        lava = Some(l);
                        break;
                    }
                    // strip_tunnel bores one axis at a time — pick the dominant axis so we
                    // approach the pool in a straight beeline (an L-path when it's diagonal).
                    let (tx, tz) = if ddx.abs() >= ddz.abs() { (ddx.signum() as i32, 0) } else { (0, ddz.signum() as i32) };
                    // STALL GUARD: no ≥1-block gain on this target in 45 s → retire it, so siting moves
                    // to the next cluster. Batch 4 rust-gym-005 sat at hd=12 "tunnelling toward (-1,0)"
                    // ~4×/s for 14+ min (1534 → 2400 s, timeout) without moving.
                    match tun_best {
                        Some((t, best, since)) if t == l => {
                            if hd < best - 1.0 {
                                tun_best = Some((l, hd, Instant::now()));
                            } else if since.elapsed() > Duration::from_secs(45) {
                                cast_debug(&format!("desc at-depth: no progress toward {l:?} in 45 s (hd {hd:.0}) — retiring it"));
                                retire_lava(l);
                                tun_best = None;
                                cluster = None;
                                continue;
                            }
                        }
                        _ => tun_best = Some((l, hd, Instant::now())),
                    }
                    cast_debug(&format!("desc at-depth y={fy} lava@{l:?} hd={hd:.0} — tunnelling toward ({tx},{tz})"));
                    tunnel_step(bot, tx, tz).await;
                } else {
                    cast_debug(&format!("desc at-depth y={fy} below={below} — no lava yet, tunnelling +Z"));
                    tunnel_step(bot, 0, 1).await;
                }
            }
            // Settle only when a source is within bucket REACH (~5) — i.e. the tunnel has
            // reached the pool edge and the bot can actually stand on the tunnel floor and
            // scoop it. Breaking at 12 (before) stopped the bot behind ~10 blocks of stone it
            // couldn't path through, so fill_bucket reached for a farther standable source and
            // no-op'd the scoop from 11 away.
            // Settle only on a pool find_fluid will actually return — it skips RETIRED pools, and
            // breaking on a retired one returned None at once and looped (rust-gym-003: 782 ×
            // `prepare: at (303,-54,669) lava=None` in one trial after the climb-out was removed).
            if feet_y(bot) <= -50 && source_lava_near(bot, 5) {
                if let Some(l) = find_fluid(bot, "lava", 24) {
                    lava = Some(l);
                    break;
                }
            }
        }
    }
    let Some(lava) = lava else {
        return None;
    };
    mem.log("cast", "lava", &format!("{},{},{}", lava.0, lava.1, lava.2));
    // Remember the pool so retries navigate STRAIGHT back (memory-first path above)
    // instead of re-descending from the surface each time — the descent eats most of
    // the 360s deadline, so without this a bot that runs out of time mid-clear starts
    // over from scratch and never accumulates enough time at the lava to finish the cast.
    mem.record(PoiKind::Lava, lava, PoiStatus::Available);

    // Get ADJACENT to the pool before anchoring. The descent spots lava from up to 24
    // blocks away (often 15+), and the edge-scan + stand-spot logic below only works when
    // the bot is actually at the pool — otherwise the frame anchors next to lava the bot
    // can't reach and the scoop finds "0 lava blocks". blocks_cant_break is already cleared,
    // so the pathfinder tunnels there (can_dig); liquid_cost keeps it out of the lava itself.
    bot.movement.blocks_cant_break.clear();
    // Target the STAND level (lava.1 + 1), not the lava cell: toward ELEVATED lava (surface above
    // the tunnel floor) a lava-level goal kept the bot a block UNDER the surface and the approach dug
    // the pool open at head height (rust-gym-003, 02:50 binary: lava surface -53, feet -54,
    // `pre-scoop heal → hp=0`). A bot that still ends below the surface abandons this pool.
    let t_app = Instant::now();
    let app = bot.goto_near(lava.0, lava.1 + 1, lava.2, 3.0).await;
    {
        // The approach logged nothing: batch 3 rust-gym-001 went from the −50 tunnel (hd 5) to feet −55
        // inside the lake in 33 s with no line between, and died. Record what the goto did.
        let p = bot.entity.position;
        cast_debug(&format!("prepare: approach goto → {:?} in {} ms, now ({:.1},{:.1},{:.1}) hp={:.0} below={}", app.as_ref().map_err(|e| e.to_string()), t_app.elapsed().as_millis(), p.x, p.y, p.z, bot.health, name_at(bot, p.x.floor() as i32, p.y.floor() as i32 - 1, p.z.floor() as i32)));
    }
    if feet_y(bot) <= lava.1 {
        cast_debug(&format!("prepare: feet {} not above the lava surface {} after the approach — retiring pool (never scoop from below)", feet_y(bot), lava.1));
        mem.mark(lava, PoiStatus::Gone);
        retire_lava(lava);
        return None;
    }
    {
        let p = bot.entity.position;
        let dist = ((lava.0 as f64 - p.x).powi(2) + (lava.2 as f64 - p.z).powi(2)).sqrt();
        cast_debug(&format!("prepare: approached lava, now {dist:.0} away at ({:.0},{:.0},{:.0})", p.x, p.y, p.z));
    }

    // Drop to the lava's LEVEL if perched above it. The descent settles as soon as an
    // exposed source is merely NEAR (within source_lava_near's radius), which over a deep
    // lava sea leaves the bot on the overburden ~10 blocks up — out of the ~4.5-block
    // bucket reach (seen live: bot at y=-45 directly over a y=-55 source, "all rounds
    // failed"). Re-approach the source column and dig down toward one block above the
    // surface; dig_down refuses to dig INTO lava, so it stops flush at feet = surface+1.
    {
        const LVL_DIRS: [(i32, i32); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];
        // Land the FEET one block above the source cell (lava.1) — a flush scoop. Live
        // proof this matters: from feet=lava.1+2 (two above) the bucket never fills
        // ('lava true->lava' every attempt); from one above it scoops (as the isolation
        // lake does). Earlier this stopped a block too high (target_level+1).
        let target_feet = lava.1 + 1;
        for _ in 0..24 {
            if feet_y(bot) <= target_feet || Instant::now() > deadline {
                break;
            }
            // SURVIVAL: bail the moment health drops descending toward the lava — a race bot
            // died here (health hit 0 dropping into a lava sea) BEFORE reaching fill_bucket's
            // guard. Climb back out and abandon this site alive with the kit rather than burn.
            if bot.health < 12.0 {
                cast_debug(&format!("prepare: ABORT descent low health={:.0} — climbing out", bot.health));
                climb_out_of_pit(bot).await;
                return None;
            }
            // STOP the instant lava is directly below the feet — that IS the flush scoop
            // level (feet one above the lava surface). Descending further submerges the bot:
            // it died at y-57 over a y-55 surface (2 blocks INTO the lava). dig_down refuses
            // lava, but descend_step / a multi-block fall can still drop the feet into the
            // column, so guard here regardless of which primitive moved us.
            {
                let fy = feet_y(bot);
                let (fx, fz) = (bot.entity.position.x.floor() as i32, bot.entity.position.z.floor() as i32);
                if is_lava(&name_at(bot, fx, fy - 1, fz)) || is_lava(&name_at(bot, fx, fy, fz)) {
                    break;
                }
                // ROOFED LAKE: the block under the feet is the lake's ceiling and the lava is one
                // further down. Digging the ceiling drops the bot onto the sea (natural deaths:
                // `pre-scoop heal → hp=0` right after `dropped to scoop level`). Stop ON the roof;
                // fill_bucket's roofed-source branch scoops from here by digging a roof block
                // 1.5–2.6 blocks away, never the one under the feet.
                if is_lava(&name_at(bot, fx, fy - 2, fz)) {
                    cast_debug(&format!("prepare: on a lake ROOF at feet {fy} (lava at {}) — stop here, scoop through the roof", fy - 2));
                    break;
                }
            }
            ensure_pickaxe(bot).await;
            bot.movement.blocks_cant_break.clear();
            let _ = bot.goto_near(lava.0, feet_y(bot), lava.2, 2.0).await; // stay above the source
            let before = feet_y(bot);
            // The fast dig_down drops through AIR under the dug block too — beside a pool that is
            // a fall into the lava layer (rust-gym-001, 14:03 binary: `dropped to scoop level y=-50
            // (lava surface -50)` → `pre-scoop heal → hp=0`; server: `tried to swim in lava`).
            // Same rule as the manual fallback below: only onto solid ground two down.
            {
                let (fx, fz, fy) = (bot.entity.position.x.floor() as i32, bot.entity.position.z.floor() as i32, feet_y(bot));
                let two = name_at(bot, fx, fy - 2, fz);
                if !is_solid(&two) || is_lava(&two) {
                    cast_debug(&format!("prepare: no solid ground 2 below feet {fy} ({two}) — stop the drop here"));
                    break;
                }
                // LAVA BESIDE the cell we'd drop into: the body would stand next to the lake. Batch 7
                // rust-gym-001 went −54 → −57 beside a −55 source and logged `pre-scoop heal → hp=0`
                // right after `dropped to scoop level`, 5×, a death-loop FAIL.
                let beside = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|&(ox, oz)| (0..=1).any(|h| is_lava(&name_at(bot, fx + ox, fy - 1 + h, fz + oz))));
                if beside {
                    cast_debug(&format!("prepare: lava beside the next cell below feet {fy} — stop the drop here"));
                    break;
                }
            }
            let mut moved = dig_down(bot).await;
            if !moved {
                // dig_down REFUSES to dig toward lava (death-avoidance) — which strands the bot
                // ~5 blocks above a deep sea, out of the ~4.5-block scoop reach, so fill_bucket
                // reports "all rounds failed" forever and the bot never fills a single lava bucket
                // (the sole wall past mining — deaths=0 now, but zero scoop progress). Manually dig
                // the block DIRECTLY below to close the last gap to the flush scoop level, but ONLY
                // when it's solid (never lava/air): the surface guard above (lava at feet-1) breaks
                // the loop one block above the lava, so this can never dig INTO the sea.
                let fy = feet_y(bot);
                let (fx, fz) = (bot.entity.position.x.floor() as i32, bot.entity.position.z.floor() as i32);
                let below = name_at(bot, fx, fy - 1, fz);
                // …and never when the block UNDER that one is lava: on a roofed lake the block
                // below the feet is the deepslate roof and digging it drops the bot onto the sea
                // (two natural deaths at "dropped to scoop level", feet = surface+1 over lava).
                // …and only onto SOLID ground: an air pocket under it dropped rust-gym-001 six
                // blocks, -50 → -56, beside a lake with its surface at -55 (dead before the scoop).
                if is_solid(&below) && !is_lava(&below) && is_solid(&name_at(bot, fx, fy - 2, fz)) && !is_lava(&name_at(bot, fx, fy - 2, fz)) {
                    ensure_pickaxe(bot).await;
                    dig_at(bot, fx, fy - 1, fz).await;
                    bot.wait_ticks(6).await.ok(); // let gravity drop the bot into the gap
                    moved = feet_y(bot) < before;
                }
            }
            if !moved {
                for &(dx, dz) in &LVL_DIRS {
                    if descend_step(bot, dx, dz).await {
                        moved = true;
                        break;
                    }
                }
            }
            if !moved || feet_y(bot) >= before {
                break; // can't get lower (lava directly below, or wedged) — scoop from here
            }
        }
        let p = bot.entity.position;
        cast_debug(&format!("prepare: dropped to scoop level y={:.0} (lava surface {})", p.y, lava.1));
    }


    // Scoop the lava bucket NOW, while we're right next to the exposed pool. The anchor +
    // chamber-clear below moves the bot away and is slow (180-cell dig loop) — it was eating
    // the deadline before the scoop, so the bot reached the lava but never filled the bucket.
    // Getting the (critical, order-sensitive) lava_bucket here first is what unblocks the cast.
    if count_items(bot, "lava_bucket") < 1 && count_items(bot, "bucket") >= 1 {
        // Fill MULTIPLE lava buckets while at this close, safe source. As the bot builds the
        // frame it drifts and the next source lands 5-8 blocks away — past the ~4.5 fluid
        // raytrace, so re-scoops "all rounds failed" and the frame stalled at 1-2 obsidian.
        // Front-load lava here, KEEPING 1 empty bucket in reserve (stop at bucket>=2). ★ The bot
        // ARRIVES at prepare with 2 empty + 1 water (race: get_water already ran; gym: prereq
        // `bucket 2`), so gating this loop at bucket>=3 (a regression — it was >=2) meant it NEVER
        // scooped a single lava with the standard provisioning → fill_bucket was never called,
        // `early scoop → lava_buckets=0` forever, no cast, no nether (whole 2026-09 wall). With >=2
        // a 2-empty bot scoops exactly 1 lava (→ 1 empty + 1 lava + 1 water), enough to cast; a
        // pour returns the empty bucket anyway, so 1 reserve covers the cup water-clear between cups.
        // FRONT-LOAD UP TO 10 lava (was 4): casting the full frame needs 10 lava pours (1 obsidian
        // each), but after block 1 the bot drifts and the source pool ends up 5-6 blocks away —
        // past the ~4.5 scoop raytrace AND unreachable by goto_near through the frame it just built
        // — so mid-frame refills fail and the frame freezes at 1/10 (the 2026-09 wall). Scooping all
        // 10 NOW, while standing at the reachable pool with a full bucket kit, lets the whole frame
        // cast with zero refills. Keeps 1 empty in reserve (bucket>=2).
        // Tolerate TRANSIENT scoop misses: a single failed fill (a flowing edge, a settle race)
        // used to `break` the front-load at 1 lava even with 24 sources left in a 5x5 pool. Retry
        // — fill_bucket re-locates the nearest source each call — and only give up after several
        // CONSECUTIVE misses (source truly exhausted / unreachable).
        // HEAL BEFORE THE FIRST SCOOP. The descent (falls, aquifers) delivers the bot hurt; the
        // pre-frame heal only runs later, and a lava nick at the scoop stand on low hp is a death
        // (gym-002 natural: arrived, one `fill try`, `EARLY BAIL DEAD`). Same bounded loop as the
        // pre-frame heal — eat until hp≥16 or food runs out.
        {
            const FOODS: [&str; 9] = [
                "cooked_beef", "cooked_porkchop", "cooked_mutton", "cooked_chicken", "cooked_rabbit",
                "bread", "beef", "porkchop", "mutton",
            ];
            let mut last_hp = bot.health;
            for _ in 0..8 {
                if bot.health >= 16.0 || bot.health <= 0.0 {
                    break;
                }
                // Burning / lava beside the body: MOVE AWAY before eating. rust-gym-003 (04:31
                // binary) arrived hurt 7 blocks from the pool and ate in place for 14 s while its
                // health went to 0 (`pre-scoop heal → hp=0`).
                if bot.health < last_hp || raw_lava_near(bot, 1) {
                    let p = bot.entity.position;
                    let (ax, az) = ((p.x - lava.0 as f64).signum() as i32, (p.z - lava.2 as f64).signum() as i32);
                    cast_debug(&format!("prepare: hurt/lava while healing (hp {:.0}) — backing off 4 blocks from the pool", bot.health));
                    let _ = bot.goto_xz(p.x.floor() as i32 + ax * 4, p.z.floor() as i32 + az * 4, 1.5).await;
                }
                last_hp = bot.health;
                let Some(food) = FOODS.iter().find(|f| count_items(bot, f) > 0) else {
                    break;
                };
                if !select_item(bot, food).await.unwrap_or(false) {
                    break;
                }
                let p = bot.entity.position;
                bot.look_at(vec3(p.x, p.y + 1.0, p.z - 3.0));
                bot.wait_ticks(2).await.ok();
                bot.activate_item().await.ok();
                bot.wait_ticks(30).await.ok();
            }
            cast_debug(&format!("prepare: pre-scoop heal → hp={:.0}", bot.health));
        }
        // Through the sealed station too (unless REFILL_LEGACY=1). The old fill_bucket scoop killed
        // rust-gym-001 twice beside one pool in batch 3: two seconds into `fill lava` it stood IN the
        // −55 surface cell at hp 10. Same goal as before: up to 10 lava, keeping 1 empty bucket.
        if std::env::var("REFILL_LEGACY").as_deref() != Ok("1") {
            let p = bot.entity.position;
            let here = (p.x.floor() as i32, feet_y(bot), p.z.floor() as i32);
            let ok = crate::tasks::portal_mold::station_refill(bot, here, Some(lava), 10, 1, false).await;
            cast_debug(&format!("prepare: early scoop (station) ok={ok} → lava_buckets={}", count_items(bot, "lava_bucket")));
        }
        let mut misses = if std::env::var("REFILL_LEGACY").as_deref() != Ok("1") { 5 } else { 0 };
        while misses < 5 && count_items(bot, "lava_bucket") < 10 && count_items(bot, "bucket") >= 2 {
            let before = count_items(bot, "lava_bucket");
            fill_bucket(bot, "lava").await;
            if count_items(bot, "lava_bucket") > before {
                misses = 0;
            } else {
                misses += 1;
                if misses >= 5 {
                    break;
                }
            }
        }
        cast_debug(&format!("prepare: early scoop → lava_buckets={}", count_items(bot, "lava_bucket")));
    }
    // Remember WHERE we scooped safely — refills return here rather than re-finding a stand.
    if count_items(bot, "lava_bucket") >= 1 {
        let p = bot.entity.position;
        *SAFE_SCOOP_STAND.lock().unwrap() = Some((p.x, p.y, p.z));
    }
    // If we STILL couldn't scoop after standing right at the pool, this lava is unscoopable from
    // here (a rim-less/capped deep sea: sources with no safe stand). RETIRE it so the memory-first
    // path won't loop straight back, and fail so the step re-descends and tunnels to fresh lava
    // elsewhere — otherwise the bot re-approaches the same sea forever ("refuse → approach → …").
    if count_items(bot, "lava_bucket") < 1 {
        cast_debug(&format!("prepare: no scoop at {lava:?} — retiring pool, will re-descend to fresh lava"));
        mem.mark(lava, PoiStatus::Gone);
        retire_lava(lava);
        // Also CLEAR a matching FORCED_LAVA — otherwise the next prepare re-adopts the very pool we
        // just retired (it's within the 48-block trust radius) and loops forever ("no scoop →
        // retire → re-adopt", 52×). A forced pool that can't be scooped (flowing-only surface lake,
        // no safe stand) must be abandoned so the bot descends to find a real SOURCE elsewhere.
        if let Some(forced) = *FORCED_LAVA.lock().unwrap() {
            let d = ((forced.0 - lava.0).pow(2) + (forced.1 - lava.1).pow(2) + (forced.2 - lava.2).pow(2)) as f64;
            if d.sqrt() < 6.0 {
                *FORCED_LAVA.lock().unwrap() = None;
            }
        }
        // No climb-out: the retired-pool list (RETIRED_LAVA) already keeps every lava finder off
        // this pool, and the at-depth steering picks the next band-level source from here. The old
        // climb_out_of_pit + 48-block walk ran ~15 silent minutes (rust-gym-002: retired at 539 s,
        // next line at 1484 s at y 38) and walked away from a half-built frame.
        return None;
    }

    // 2. Anchor the frame beside the scoop spot (see below — the mold builds its own pad).
    bot.set_control_state("sneak", false);
    // Re-anchor AT the pool, not wherever the scoop left the bot: fill_bucket can climb a tall cave
    // lava column while scooping and drift the bot several blocks up (seen at a real pool: scooped
    // at y5 but ended at y13). Anchoring there floats the whole frame high above the pool and every
    // cup pour lands as air. Walk back to the lava and drop to its level so the frame anchors AT the
    // pool, where refills are in reach and the platform can cap from the pool's own rim.
    // Only re-approach if the bot actually DRIFTED UP off the pool (feet well above the lava);
    // when it's already at the pool level (e.g. the DEEPSEA island / a normal scoop), navigating
    // back INTO the lava just walks it into the sea and it burns to death refilling (regressed the
    // 8/10 deep-sea run to a death at block 2). The re-approach is only for the tall-column drift.
    if feet_y(bot) > lava.1 + 3 {
        bot.movement.blocks_cant_break.clear();
        let _ = bot.goto_near(lava.0, lava.1 + 1, lava.2, 3.0).await;
        descend_to_y(bot, lava.1 + 1).await;
    }
    let feet0 = (lava.1 + 1).min(feet_y(bot));
    let px = bot.entity.position.x.floor() as i32;
    let pz = bot.entity.position.z.floor() as i32;
    // Anchor the frame AT THE SCOOP STAND. The template mold (portal_mold.rs) terraforms its own
    // pad + rim, digs its own frame cells and headroom, and shifts its origin off any lava in the
    // footprint — the chamber-excavation / cobble-platform code that used to live here (~240 lines)
    // built a work area for the deleted per-cell cast and is redundant with the mold.
    // Still AT the pool? A death during the early scoop respawns the bot on the surface, and the
    // anchor below would put the frame there (rust-gym-002: `MOLD start anchor=(961,113,1463)`, the
    // pool at y -55). Far from the pool → return; the next pass descends again with the buckets.
    {
        let p = bot.entity.position;
        // VERTICAL distance: a respawn lands on the surface (tens of blocks up); scooping from a far
        // edge of a big pool is legit (false positive: 12 blocks away horizontally, same depth).
        let d = (p.y - lava.1 as f64).abs();
        if d > 6.0 {
            cast_debug(&format!("prepare: {d:.0} blocks above/below the pool {lava:?} after the scoop (death/respawn?) — not anchoring here"));
            return None;
        }
    }
    let (bx, by, bz) = (px, feet0, pz);
    cast_debug(&format!("prepare: anchor at scoop stand ({bx},{by},{bz}) — the mold builds its own pad"));
    // Plug the roof holes the scoop left: roofed-source scoops dig the roof open and the pool
    // refills it, leaving open lava at the stand's level beside the frame. rust-gym-003 (10:45
    // binary) died 9 s after `MOLD layer 1: platform done`; probe: (691,-55,705) lava, open to air.
    {
        let fy = feet_y(bot);
        let mut holes = Vec::new();
        for dy in [-1, 0] {
            for dx in -3..=3 {
                for dz in -3..=3 {
                    let c = (px + dx, fy + dy, pz + dz);
                    // Only ABOVE the pool's lava level: roof holes are above it by definition;
                    // below it is the lake body, and sealing that starved rust-gym-002's refills
                    // (11:2x: plugs at −55/−56, then `remembered lava … dry on arrival`, 8/10).
                    if c.1 > lava.1 && name_at(bot, c.0, c.1, c.2) == "lava" && !solid_at(bot, c.0, c.1 + 1, c.2) {
                        holes.push(c);
                    }
                }
            }
        }
        for c in holes {
            let ok = place_cobble(bot, c).await;
            cast_debug(&format!("prepare: plug open lava {c:?} → {ok}"));
        }
    }

    // 4. Top up a lava bucket from the pool.
    {
        let p = bot.entity.position;
        cast_debug(&format!(
            "prepare pre-fill: bot=({:.0},{:.0},{:.0}) lava=({},{},{}) exp16={} exp24={} exp32={}",
            p.x, p.y, p.z, lava.0, lava.1, lava.2,
            bot.find_exposed_blocks("lava", 16, 64).len(),
            bot.find_exposed_blocks("lava", 24, 64).len(),
            bot.find_exposed_blocks("lava", 32, 64).len()
        ));
    }
    // Legacy only: with the station, a 0-lava early scoop must re-site, not improvise with the old
    // fill_bucket (the path that killed rust-gym-001 twice beside one pool in batch 3).
    if std::env::var("REFILL_LEGACY").as_deref() == Ok("1") && count_items(bot, "lava_bucket") < 1 && count_items(bot, "bucket") >= 1 {
        fill_bucket(bot, "lava").await;
    }
    // Return to the frame anchor (precisely) so build_nether_portal anchors there. The anchor is now
    // usually the station stand, whose open side O is air over lava. A jumping `walk_to_xz` (real ticks)
    // from 0.5 off killed batch 6b rust-gym-002 right after `prepare pre-fill`. Already close → a
    // sneaking, non-jumping settle (edge-safe), as in cast_cell. Farther → pathfinder first.
    {
        let p = bot.entity.position;
        let off = ((bx as f64 + 0.5 - p.x).powi(2) + (bz as f64 + 0.5 - p.z).powi(2)).sqrt();
        if off > 1.5 || feet_y(bot) != by {
            let _ = bot.goto_near(bx, by, bz, 1.0).await;
        }
        crate::tasks::portal_mold::settle_xz(bot, bx as f64 + 0.5, bz as f64 + 0.5, 0.3, 60).await;
        bot.set_control_state("sneak", false);
    }
    // Return the pool location so casts can navigate BACK to it to refill lava
    // (re-scanning from wherever a cast left the bot is what wedged the descend-loop).
    if count_items(bot, "lava_bucket") >= 1 {
        Some(lava)
    } else {
        None
    }
}

/// Build + light a 4x5 (10-obsidian, no-corner) nether portal by casting.
pub async fn build_nether_portal(bot: &mut Bot<'_>, mem: &mut WorldMemory) -> StepResult {
    // DISPLACEMENT RECOVERY: an uncaught lava death respawns the bot at WORLD SPAWN (a deep frame's
    // spawnpoint is rejected by the server as unsafe), thousands of blocks from a partial frame. On
    // the step's re-run, prepare_cast_site would then re-hunt lava at world spawn and ORPHAN the
    // N/10 frame (live: `POS FAIL off=5919`, frame stuck at 4/10 forever). If a frame anchor persists
    // and we're far from it, tp straight back (the bot is op'd) so the frame RESUMES instead of
    // starting over. Copy the Option out first (drop the mutex guard before awaiting).
    // Load the anchor from DISK if the in-memory one is empty — i.e. a FRESH process right after a
    // relaunch. Without this the check saw None, skipped, and prepare re-hunted lava at the SPAWN
    // lane (live: 004 prepared at 864,668, dug a chamber, and died there) — orphaning its 8/10
    // frame for a whole attempt until the later anchor-reuse finally loaded the file.
    let mut anchor_now = *FRAME_ANCHOR.lock().unwrap();
    if anchor_now.is_none() {
        anchor_now = load_frame_anchor();
        if anchor_now.is_some() {
            *FRAME_ANCHOR.lock().unwrap() = anchor_now;
        }
    }
    // Cycle 5, Phase 1 (ii): count re-sites after a death (should read 0). Instrumentation only: a
    // frame anchor that existed before a death and is gone after it means the bot started a new site.
    {
        use std::sync::atomic::Ordering::Relaxed;
        if anchor_now.is_some() {
            if !HAD_ANCHOR.swap(true, Relaxed) {
                DEATHS_AT_ANCHOR.store(bot.deaths, Relaxed);
            }
        } else if HAD_ANCHOR.load(Relaxed) && bot.deaths > DEATHS_AT_ANCHOR.load(Relaxed) {
            let n = RESITE_AFTER_DEATH.fetch_add(1, Relaxed) + 1;
            HAD_ANCHOR.store(false, Relaxed);
            mem.log("cast", "resite_after_death", &format!("{n}"));
            cast_debug(&format!("build: RE-SITE AFTER DEATH #{n}: the frame anchor did not survive the death"));
        }
    }
    if let Some((ax, ay, az)) = anchor_now {
        let p = bot.entity.position;
        let d = ((ax as f64 - p.x).powi(2) + (az as f64 - p.z).powi(2)).sqrt();
        if d > 16.0 {
            cast_debug(&format!("build: displaced {d:.0} from frame anchor — tp back to resume"));
            // Same as respawn_at_frame: land on a VERIFIED safe stand, not the (lava-cup) anchor cell.
            let (sx, sy, sz) = safe_stand_near_frame(bot, ax, ay, az);
            let _ = bot.run_command(&format!("tp @s {sx} {sy} {sz}")).await;
            bot.wait_ticks(10).await.ok();
        }
        // SWEEP STRAY LAVA at the frame on resume. A pour-miss from an earlier attempt can leave a
        // lava SOURCE on the lane (live: (901,-50,586)) — it burns the bot, and worse, find_fluid
        // then picks it as "the pool" and anchors the cast-site chamber right there (895,-49,583),
        // 3 blocks off the real frame. Cap exposed lava in the lane + margins; SKIP the frame plane
        // (dz=0) so a stale cup is left to the cast's own cup logic rather than sealed with cobble.
        let mut swept = 0;
        for dx in -1..=4 {
            for dy in 0..=5 {
                for dz in [-1, 1, 2, 3] {
                    let (x, y, z) = (ax + dx, ay + dy, az + dz);
                    if is_lava(&name_at(bot, x, y, z)) && count_items(bot, &build_block(bot)) > 0 {
                        place_cobble(bot, (x, y, z)).await;
                        swept += 1;
                    }
                }
            }
        }
        if swept > 0 {
            cast_debug(&format!("build: swept {swept} stray lava around the frame before resuming"));
        }
    }
    // Already cast?
    let mut lava_pool: Option<(i32, i32, i32)> = None;
    // Cycle 5 (steve's cycle-4 finding): "already cast" is read off the STORED mold's 10 cells, never a
    // radius count — natural obsidian (lava meeting water, a ruined portal) within 8 blocks made the
    // count skip prepare + cast, then frame_check on the anchor failed, the same way every attempt.
    let stored = (*FRAME_ANCHOR.lock().unwrap()).or_else(load_frame_anchor);
    let anchored_present = stored.map(|a| anchored_frame_present(bot, a)).unwrap_or(0);
    let near_count = bot.find_blocks("obsidian", 8, 12).len();
    if near_count >= 10 && anchored_present < 10 {
        cast_debug(&format!("build: {near_count} obsidian within 8 but the stored mold reads {anchored_present}/10 — not treated as cast"));
    }
    if anchored_present >= 10 {
        // Frame complete — fall through to lighting. KEEP the anchor: clearing it here made the
        // next lines anchor a brand-new frame at the bot's feet when lighting failed once
        // (gym-003: `frame_check 10/10` → `portal_start` one block over → a second mold). The
        // anchor is released on the success path, after the portal is lit.
    } else {
        if scaffold_count(bot) < 30 {
            return failure("need ~30 cobble/dirt to scaffold the cast");
        }
        lava_pool = prepare_cast_site(bot, mem).await;
        if lava_pool.is_none() {
            return failure("no lava pool found / lava bucket not filled");
        }
        if count_items(bot, "lava_bucket") < 1 {
            return failure("need a lava bucket to cast obsidian");
        }
        if count_items(bot, "water_bucket") + count_items(bot, "bucket") < 1 {
            return failure("need a water/empty bucket to cast obsidian");
        }
    }

    // PERSIST the frame anchor: reuse the stored one (walk back to it) so obsidian accumulates in
    // ONE frame across respawns/relocations, instead of anchoring a fresh empty frame each call.
    let (bx, by, bz) = {
        // Prefer the in-memory anchor; if it was wiped by a relaunch, reload it from the file so
        // the bot resumes its partial frame instead of starting over (see FRAME_ANCHOR).
        let mut anchor = *FRAME_ANCHOR.lock().unwrap();
        if anchor.is_none() {
            anchor = load_frame_anchor();
            if anchor.is_some() {
                *FRAME_ANCHOR.lock().unwrap() = anchor;
            }
        }
        if let Some(a) = anchor {
            let _ = bot.goto_near(a.0, a.1, a.2, 2.0).await;
            a
        } else {
            let a = (
                bot.entity.position.x.floor() as i32,
                bot.entity.position.y.floor() as i32,
                bot.entity.position.z.floor() as i32,
            );
            *FRAME_ANCHOR.lock().unwrap() = Some(a);
            save_frame_anchor(a);
            a
        }
    };
    // Respawn AT the frame (not the surface) so a fire death RESUMES casting immediately instead
    // of re-descending ~100 blocks to the band each life — that descent overhead was capping the
    // build at ~1 obsidian per life across the churn. Idempotent; the bot is op'd.
    let _ = bot.run_command(&format!("spawnpoint @s {bx} {} {bz}", by + 1)).await;
    let at = |dx: i32, dy: i32| (bx + dx, by + dy, bz);

    // Frame: bottom (x=1,2), left column (x=0,y1..3), right (x=3,y1..3), top (x=1,2,y4).
    let mut frame: Vec<(i32, i32, i32)> = vec![at(1, 0), at(2, 0)];
    for dy in 1..=3 {
        frame.push(at(0, dy));
    }
    for dy in 1..=3 {
        frame.push(at(3, dy));
    }
    frame.push(at(1, 4));
    frame.push(at(2, 4));

    mem.log("cast", "portal_start", &format!("{bx},{by},{bz}"));
    // GYM_FULLFRAME=1 pre-places ALL 10 frame cells + clears the interior, so the cast is skipped
    // entirely and only the DOWNSTREAM runs (flint&steel light + walk-through → nether). Validates
    // the rest of the pipeline end-to-end, isolating the top-row CAST as the sole remaining piece.
    if std::env::var("GYM_FULLFRAME").is_ok() {
        for &p in &frame {
            let _ = bot.run_command(&format!("setblock {} {} {} minecraft:obsidian", p.0, p.1, p.2)).await;
        }
        // Clear the 2x3 interior (x=bx+1..bx+2, y=by..by+2, z=bz) so the portal can light.
        for ix in 1..=2 {
            for iy in 0..=2 {
                let _ = bot.run_command(&format!("setblock {} {} {} minecraft:air", bx + ix, by + iy, bz)).await;
            }
        }
        bot.wait_ticks(10).await.ok();
        cast_debug("GYM_FULLFRAME: pre-placed all 10 obsidian + cleared interior — testing light+enter only");
    }
    let is_obsidian_at = |bot: &Bot, p: (i32, i32, i32)| name_at(bot, p.0, p.1, p.2) == "obsidian";
    // HEAL TO FULL before the 10-block cast. A deep-sea gym reproduce cast obsidian cleanly at
    // hp=20 (0 deaths) — the cast WORKS at full health. But a RACE bot arrives here DAMAGED from
    // the ~100-block descent to the lava sea and burns to 0 mid-cast (ABORT health=0 after eating
    // 18x). Start the cast from full health so it has the headroom. eat_if_hurt skips near lava,
    // but the platform IS near lava, so eat DIRECTLY — the bot is on solid cobble (won't fall in)
    // and food only tops hunger, so a short regen wait after each meal climbs health. Bounded.
    {
        const FOODS: [&str; 9] = [
            "cooked_beef", "cooked_porkchop", "cooked_mutton", "cooked_chicken", "cooked_rabbit",
            "bread", "beef", "porkchop", "mutton",
        ];
        for _ in 0..12 {
            if bot.health >= 18.0 {
                break;
            }
            let Some(food) = FOODS.iter().find(|f| count_items(bot, f) > 0) else {
                break; // no food — nothing to do
            };
            if !select_item(bot, food).await.unwrap_or(false) {
                break;
            }
            let p = bot.entity.position;
            bot.look_at(vec3(p.x, p.y + 1.0, p.z + 3.0));
            bot.wait_ticks(2).await.ok();
            bot.activate_item().await.ok();
            bot.wait_ticks(30).await.ok(); // let natural regen (foodLevel>=18) tick health up
        }
        cast_debug(&format!("cast: pre-frame heal → hp={:.0}", bot.health));
        // Dead at the stand (it stood on a spill): don't start the mold as a corpse —
        // rust-gym-002 (11:34 binary) `pre-frame heal → hp=0` then `MOLD start` … `MOLD dead` 88 s later.
        if bot.health <= 0.0 {
            respawn_at_frame(bot).await;
            return failure("dead at the scoop stand before the mold".to_string());
        }
    }
    // TEMPLATE MOLD: one fixed geometry for all 10 cells — see portal_mold.rs. (The per-cell
    // cast it replaced — cups, bowls, pillars, side-casts, lid handling — was deleted once the
    // mold cast 10/10 on every gym arena.)
    {
        let made = crate::tasks::portal_mold::cast_frame_mold(bot, (bx, by, bz), lava_pool, mem).await;
        if !made {
            let done = frame.iter().filter(|p| is_obsidian_at(bot, **p)).count();
            mem.log("cast", "mold_incomplete", &format!("{done}/10"));
            if bot.health <= 0.0 {
                respawn_at_frame(bot).await;
            }
            // Nothing cast and no lava to cast with: this site is dead. Drop the anchor and retire
            // its pool so the next attempt re-sites. Resuming it re-entered the mold about once a
            // second (batch 5 rust-gym-002: `MOLD end 0/10` hot loop after `STATION none`).
            if done == 0 && count_items(bot, "lava_bucket") == 0 {
                *FRAME_ANCHOR.lock().unwrap() = None;
                if let Some(l) = lava_pool {
                    mem.mark(l, PoiStatus::Gone);
                    retire_lava(l);
                }
                cast_debug("cast: 0/10 and no lava — site abandoned, pool retired, re-siting");
            }
            return failure(format!("mold cast incomplete at {done}/10"));
        }
    }
    // The mold may have SHIFTED the origin off the lava pool (it persists the new anchor). Every
    // step below — interior clear, frame_check, lighting — must use the frame that was actually
    // cast: with the stale origin two 10/10 casts read `frame_check 2/10` and the step re-ran.
    let (bx, by, bz) = (*FRAME_ANCHOR.lock().unwrap()).unwrap_or((bx, by, bz));
    let at = |dx: i32, dy: i32| (bx + dx, by + dy, bz);
    let frame: Vec<(i32, i32, i32)> = {
        let mut f = vec![at(1, 0), at(2, 0)];
        f.extend((1..=3).map(|dy| at(0, dy)));
        f.extend((1..=3).map(|dy| at(3, dy)));
        f.push(at(1, 4));
        f.push(at(2, 4));
        f
    };

    // Open the 2x3 interior + the +Z approach — it MUST be pure air or the portal won't light.
    // Two things clutter it: build_inner_fill's SCAFFOLD cobble, and WATER (each cup's water
    // bowl sits one block above the cast block, so the bottom-row bowls land IN the interior).
    // dig_at removes solids but NOT water, so scoop any water with an empty bucket and dig any
    // solid (never obsidian). Reposition per cell so the bot can actually reach it, and retry a
    // few times as flowing water settles — an uncleared interior is exactly "would not light".
    for _pass in 0..4 {
        descend_to_y(bot, by).await;
        let mut all_air = true;
        for dx in 1..=2 {
            for dy in 1..=3 {
                for dz in [0, 1] {
                    let c = (bx + dx, by + dy, bz + dz);
                    let n = name_at(bot, c.0, c.1, c.2);
                    if is_air(&n) || n == "nether_portal" {
                        continue;
                    }
                    let _ = bot.goto_near(bx + dx, by, bz + 2, 2.0).await;
                    if n.contains("water") {
                        if count_items(bot, "bucket") >= 1 {
                            select_item(bot, "bucket").await.ok();
                            reliable_use(bot, vec3(c.0 as f64 + 0.5, c.1 as f64 + 0.5, c.2 as f64 + 0.5)).await;
                        }
                    } else if n != "obsidian" {
                        dig_at(bot, c.0, c.1, c.2).await;
                    }
                    let now = name_at(bot, c.0, c.1, c.2);
                    if !is_air(&now) && now != "nether_portal" {
                        all_air = false;
                    }
                }
            }
        }
        cast_debug(&format!("interior pass {_pass}: all_air={all_air}"));
        if all_air {
            break;
        }
    }

    let present = frame.iter().filter(|p| name_at(bot, p.0, p.1, p.2) == "obsidian").count();
    mem.log("cast", "frame_check", &format!("{present}/10"));
    if present < 10 {
        return failure(format!("only {present}/10 obsidian present"));
    }

    // Light it: flint & steel on a bottom frame block's top face.
    if count_items(bot, "flint_and_steel") < 1 {
        return failure("frame cast but no flint & steel to light it");
    }
    // Stands, in order: right in front at pad level; the rim row one block up (from there the
    // ray to the bottom row's top face clears any leftover tower stubs); the back row. Each
    // stand is tried against both bottom cells.
    for (sy, sz) in [(by, bz + 2), (by + 1, bz + 3), (by + 1, bz + 2)] {
        for lit in [at(1, 0), at(2, 0)] {
            let _ = bot.goto_near(lit.0, sy, sz, 1.0).await;
            select_item(bot, "flint_and_steel").await.ok();
            bot.look_at(vec3(lit.0 as f64 + 0.5, lit.1 as f64 + 1.0, lit.2 as f64 + 0.5));
            bot.wait_ticks(3).await.ok();
            let _ = bot.place_block(lit.0, lit.1, lit.2, Face::Top).await;
            // Poll ANY interior cell for up to 60 ticks: rust-gym-001 (13:17 binary) logged `light:
            // no portal` from all three stands while the server showed all four probed interior
            // cells as nether_portal — a single-cell read after 20 ticks lost the race.
            let lit_now = |bot: &Bot| (1..=2).any(|dx| (1..=3).any(|dy| name_at(bot, bx + dx, by + dy, bz) == "nether_portal"));
            for _ in 0..6 {
                bot.wait_ticks(10).await.ok();
                if lit_now(bot) {
                    break;
                }
            }
            if lit_now(bot) {
                mem.log("cast", "portal_lit", &format!("{bx},{by},{bz}"));
                clear_frame_anchor(); // done with this frame; a future portal re-anchors fresh
                return success(format!("nether portal cast & lit at {bx},{by},{bz}"));
            }
        }
        let p = bot.entity.position;
        cast_debug(&format!("light: no portal from stand ({sy},{sz}) — bot=({:.1},{:.1},{:.1})", p.x, p.y, p.z));
    }
    failure(format!("frame cast ({present}/10) but portal would not light"))
}

/// Walk into the lit portal and wait for the dimension change.
pub async fn enter_nether(bot: &mut Bot<'_>) -> StepResult {
    let Some(portal) = bot.find_block("nether_portal", 64) else {
        return failure("no portal found to enter");
    };
    let start_dim = bot.game.dimension.clone();
    // Step into the portal block and wait for the server to teleport us.
    for _ in 0..6 {
        let _ = bot.goto_near(portal.0, portal.1, portal.2, 0.0).await;
        for _ in 0..20 {
            bot.wait_ticks(10).await.ok();
            if bot.game.dimension != start_dim {
                return success(format!("entered the nether ({})", bot.game.dimension));
            }
        }
        // Nudge into the portal manually if pathing stopped short.
        bot.look_at(vec3(portal.0 as f64 + 0.5, portal.1 as f64, portal.2 as f64 + 0.5));
        bot.set_control_state("forward", true);
        // Ten real game ticks (`drive_tick` counts packets: the nudge lasted milliseconds).
        bot.wait_ticks(10).await.ok();
        bot.clear_control_states();
        if bot.game.dimension != start_dim {
            return success(format!("entered the nether ({})", bot.game.dimension));
        }
    }
    failure("stood in portal but no dimension change")
}
