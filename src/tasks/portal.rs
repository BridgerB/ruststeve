//! Nether portal by obsidian casting (no diamond pickaxe) + portal entry.
//!
//! Port of steve's `tasks/portal/{cast,enter}.ts`. Each obsidian block is cast in
//! place: a fully-enclosed 1-block cup holds a lava source, then water poured into
//! the block directly above flows down and turns it to obsidian. The cup contains
//! the lava so it never reaches the bot. The 4x5 frame (10 obsidian, no corners) is
//! cast bottom-up, the 2x3 interior is dug out, and it's lit with flint & steel.

use std::time::{Duration, Instant};

use crate::bot::{Bot, DriveStep, Face};
use crate::vec3::{vec3, Vec3};

use crate::bot_utils::{count_items, select_item};
use crate::memory::{PoiKind, PoiStatus, WorldMemory};
use crate::tasks::mining::{descend_step, dig_down, ensure_pickaxe, strip_tunnel};
use crate::types::{failure, success, StepResult};

// ── block classification ────────────────────────────────────────────────────

/// Debug log for the cast (to stderr → the bot log) when CRAFT_DEBUG is set.
fn cast_debug(msg: &str) {
    if std::env::var("CRAFT_DEBUG").is_ok() {
        eprintln!("    CAST {msg}");
    }
}

/// Block name at (x,y,z), or "air" when empty.
fn name_at(bot: &Bot, x: i32, y: i32, z: i32) -> String {
    bot.block_at(x, y, z).map(|b| b.name).unwrap_or_else(|| "air".into())
}

fn is_air(n: &str) -> bool {
    n == "air" || n == "cave_air" || n == "void_air"
}

const SOFT: &[&str] = &["short_grass", "tall_grass", "fern", "snow", "snow_layer", "dead_bush"];

fn is_replaceable(n: &str) -> bool {
    is_air(n) || SOFT.contains(&n)
}

fn is_solid(n: &str) -> bool {
    !is_replaceable(n) && !n.contains("water") && !n.contains("lava")
}

fn is_lava(n: &str) -> bool {
    n == "lava" || n == "flowing_lava"
}

/// Is (x,y,z) a SOURCE fluid block (level=0)? Only source blocks can be picked up with a
/// bucket — flowing fluid (level 1-8) scoops nothing. The lava-search finds cave-edge lava
/// that's frequently FLOWING, so the scoop failed ("all rounds failed") even at 1 block away;
/// targeting source blocks fixes it. Missing/empty level is treated as source (still a lake).
fn is_fluid_source(bot: &Bot, x: i32, y: i32, z: i32, fluid: &str) -> bool {
    bot.block_at(x, y, z)
        .map(|b| {
            (b.name == fluid || b.name.contains(fluid))
                && b.properties.get("level").map(|l| l == "0").unwrap_or(true)
        })
        .unwrap_or(false)
}

fn solid_at(bot: &Bot, x: i32, y: i32, z: i32) -> bool {
    is_solid(&name_at(bot, x, y, z))
}

/// Is there ANY lava block (exposed or sealed) within `r` of the bot? Cheap raw scan
/// used to STOP descending before the bot blunders into a lava lake — dig_down and
/// descend_step avoid lava, but the relocate goto_xz will happily path a bot straight
/// through one at depth (with blocks_cant_break cleared). Near lava, scoop; don't walk.
fn raw_lava_near(bot: &Bot, r: i32) -> bool {
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

fn feet_y(bot: &Bot) -> i32 {
    bot.entity.position.y.floor() as i32
}

/// The cheap throwaway block we scaffold/mould with. cobbled_deepslate counts: the
/// cast happens at lava depth (y≈-50) where mined stone is DEEPSLATE → cobbled_deepslate,
/// not cobblestone. Ignoring it stranded portal-ready bots looping "need ~30 cobble"
/// with a full stack of deepslate cobble the step gate (which counts both) let through.
fn build_block(bot: &Bot) -> &'static str {
    if count_items(bot, "cobblestone") > 0 {
        "cobblestone"
    } else if count_items(bot, "cobbled_deepslate") > 0 {
        "cobbled_deepslate"
    } else {
        "dirt"
    }
}

/// Total throwaway scaffold blocks on hand (cobble of either kind + dirt).
fn scaffold_count(bot: &Bot) -> i32 {
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
async fn reliable_use(bot: &mut Bot<'_>, look: Vec3) {
    bot.look_at(look);
    bot.wait_ticks(7).await.ok();
    bot.activate_item().await.ok();
    bot.wait_ticks(15).await.ok();
}

/// Dig the block at (x,y,z) unless it's air or obsidian (an iron pick can't break
/// obsidian and the dig would hang).
async fn dig_at(bot: &mut Bot<'_>, x: i32, y: i32, z: i32) {
    let n = name_at(bot, x, y, z);
    if is_air(&n) || n == "obsidian" || n.contains("lava") {
        return;
    }
    bot.look_at(vec3(x as f64 + 0.5, y as f64 + 0.5, z as f64 + 0.5));
    bot.wait_ticks(2).await.ok();
    let _ = bot.dig(x, y, z).await;
    bot.wait_ticks(3).await.ok();
}

/// Place a build block at `pos` against any solid neighbour. True once solid.
async fn place_cobble(bot: &mut Bot<'_>, pos: (i32, i32, i32)) -> bool {
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
        bot.wait_ticks(3).await.ok();
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
async fn eat_if_hurt(bot: &mut Bot<'_>) {
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
async fn ensure_solid(bot: &mut Bot<'_>, pos: (i32, i32, i32), depth: i32) -> bool {
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
async fn walk_to_xz(bot: &mut Bot<'_>, tx: f64, tz: f64, target_dist: f64, max_ticks: u32) {
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
        let lava_cell = |bot: &Bot, dx: i32, dz: i32| {
            name_at(bot, px + dx, py, pz + dz).contains("lava")
                || name_at(bot, px + dx, py - 1, pz + dz).contains("lava")
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
        if bot.drive_tick().await.map(|s| matches!(s, DriveStep::Disconnected)).unwrap_or(true) {
            break;
        }
        bot.set_control_state("jump", false);
    }
    bot.set_control_state("forward", false);
    bot.set_control_state("jump", false);
}

/// Dig straight down (every cell under the footprint) until feet reach `target_y`.
async fn descend_to_y(bot: &mut Bot<'_>, target_y: i32) {
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
async fn pillar_up(bot: &mut Bot<'_>, target_y: i32) -> bool {
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
    let positions = bot.find_exposed_blocks(fluid, max_dist, 64);
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
async fn fill_bucket(bot: &mut Bot<'_>, fluid: &str) -> bool {
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
    // flush stand; PLACING a cobble to build a flush stand; recessed only as a last resort.
    let mut flush: Option<((i32, i32, i32), (f64, f64, f64))> = None;
    // (source, block-to-place-at, stand)
    let mut place: Option<((i32, i32, i32), (i32, i32, i32), (f64, f64, f64))> = None;
    let mut recessed: Option<((i32, i32, i32), (f64, f64, f64))> = None;
    let mut any_source: Option<(i32, i32, i32)> = None; // nearest source w/ air above (pillar fallback)
    let mut source_count = 0u32;
    for src in candidates {
        if !is_fluid_source(bot, src.0, src.1, src.2, fluid) {
            continue; // only ever anchor the scoop on a real source block
        }
        if !is_air(&name_at(bot, src.0, src.1 + 1, src.2)) {
            continue; // need an open surface to scoop
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
            // Placeable flush: an air gap at source level with a solid base to place a
            // cobble against — build the flush stand the terrain didn't provide (lava only;
            // water is easy to scoop and shouldn't waste blocks near it).
            if place.is_none()
                && fluid == "lava"
                && is_air(&name_at(bot, sx, src.1, sz))
                && solid_at(bot, sx, src.1 - 1, sz)
                && is_air(&name_at(bot, sx, src.1 + 1, sz))
                && is_air(&name_at(bot, sx, src.1 + 2, sz))
            {
                place = Some((src, (sx, src.1, sz), (sx as f64 + 0.5, (src.1 + 1) as f64, sz as f64 + 0.5)));
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
    } else if let Some((src, block, stand)) = place {
        cast_debug(&format!("fill {fluid}: placing flush stand at {block:?}"));
        let _ = bot.goto_near(block.0, block.1, block.2, 2.0).await;
        if select_item(bot, build_block(bot)).await.unwrap_or(false) {
            let _ = bot.place_block(block.0, block.1 - 1, block.2, Face::Top).await;
            bot.wait_ticks(3).await.ok();
        }
        (src, stand)
    } else if let Some(r) = recessed {
        r
    } else if let (Some(s), "lava") = (any_source, fluid) {
        // Terrain gave no stand spot beside a source (a pool over a cave, all-lava rims).
        // Build one: stand at a non-lava neighbour column and pillar up so our feet sit
        // one block above the source, then scoop down at a shallow angle.
        cast_debug(&format!("fill lava: no stand — pillaring to source {s:?} ({source_count} sources)"));
        // Require a neighbour with a SOLID floor to stand ON (feet at s.1+1). The old
        // `.unwrap_or((s.0, s.2))` fell back to the SOURCE's own column when every neighbour was
        // lava — pillaring the bot directly onto the lava source (20→0 hp). If no safe neighbour
        // exists (interior source, all-lava rim), refuse this fill rather than stand on lava; the
        // caller retries / navigates to the remembered pool, and a rim source is used next.
        let Some((nx, nz)) = dirs8.iter().map(|(dx, dz)| (s.0 + dx, s.2 + dz)).find(|&(nx, nz)| {
            !is_lava(&name_at(bot, nx, s.1, nz))
                && !is_lava(&name_at(bot, nx, s.1 - 1, nz))
                && solid_at(bot, nx, s.1 - 1, nz)
        }) else {
            cast_debug(&format!("fill lava: no SAFE stand for {s:?} — refuse (won't pillar onto source)"));
            return false;
        };
        bot.movement.blocks_cant_break.clear();
        let _ = bot.goto_near(nx, s.1, nz, 2.0).await;
        pillar_up(bot, s.1 + 1).await;
        bot.set_control_state("sneak", false);
        (s, (nx as f64 + 0.5, (s.1 + 1) as f64, nz as f64 + 0.5))
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
        if bot.health < 8.0 {
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
        walk_to_xz(bot, stand.0, stand.2, 0.4, 50).await;
        // DESCEND to the source's level if the stand nav left the bot perched ABOVE it. A scoop is a
        // short raycast; when the source sits well below the bot (e.g. the bot was tp'd to y-41 but
        // the scoopable source is in the deep sea at y-55) goto_near stops at the higher lip and the
        // bot scoops from 14 blocks up (`hdist=14.9 lava true->lava` → "all rounds failed" → retire).
        // Drop to one above the source and re-pin, so the aim is a shallow reach into the surface.
        if feet_y(bot) > src.1 + 2 {
            bot.movement.blocks_cant_break.clear();
            descend_to_y(bot, src.1 + 1).await;
            let _ = bot.goto_near(stand.0 as i32, (src.1 + 1).max(feet_y(bot)), stand.2 as i32, 1.0).await;
            walk_to_xz(bot, stand.0, stand.2, 0.4, 40).await;
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
            if is_fluid_source(bot, t.0, t.1, t.2, fluid) {
                targets.push(t);
            }
        }
        if targets.is_empty() {
            targets.push(src);
        }
        {
            let p = bot.entity.position;
            let d = ((src.0 as f64 - p.x).powi(2) + (src.2 as f64 - p.z).powi(2)).sqrt();
            let lvl = bot.block_at(src.0, src.1, src.2).and_then(|b| b.properties.get("level").cloned());
            cast_debug(&format!(
                "fill try r{round}: src={src:?}({}) lvl={lvl:?} targets={} bot=({:.1},{:.1},{:.1}) hdist={d:.1}",
                name_at(bot, src.0, src.1, src.2), targets.len(), p.x, p.y, p.z
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
            let above = name_at(bot, t.0, t.1 + 1, t.2);
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
                    cast_debug(&format!("  approach source {t:?}: {horiz:.1} away → goto_near"));
                    let _ = bot.goto_near(t.0, t.1 + 1, t.2, 1.8).await;
                    // goto_near often can't path the lava-band maze, leaving the bot 4-5 away
                    // (past the ~4.5 fluid raytrace → "lava true->lava" every attempt). DIG a
                    // straight 2-block path toward the source at feet+head so the ray gets in
                    // reach. Never dig the lava itself (that would flood/burn); then re-approach.
                    let p2 = bot.entity.position;
                    let still = ((t.0 as f64 + 0.5 - p2.x).powi(2) + (t.2 as f64 + 0.5 - p2.z).powi(2)).sqrt();
                    if still > 3.3 {
                        let feet = feet_y(bot);
                        let (bx2, bz2) = (p2.x.floor() as i32, p2.z.floor() as i32);
                        let dx = (t.0 - bx2).signum();
                        let dz = (t.2 - bz2).signum();
                        for step in 1..=2 {
                            let (nx, nz) = (bx2 + dx * step, bz2 + dz * step);
                            for ny in [feet, feet + 1] {
                                let b = name_at(bot, nx, ny, nz);
                                if is_solid(&b) && !is_lava(&b) {
                                    dig_at(bot, nx, ny, nz).await;
                                }
                            }
                        }
                        let _ = bot.goto_near(t.0, t.1 + 1, t.2, 1.8).await;
                    }
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
                {
                    let p = bot.entity.position;
                    let (fx, fz) = (p.x.floor() as i32, p.z.floor() as i32);
                    if is_lava(&name_at(bot, fx, feet_y(bot) - 1, fz)) {
                        cast_debug(&format!("fill {fluid}: FOOTING on lava at ({:.1},{:.1},{:.1}) — retreat, no scoop", p.x, p.y, p.z));
                        pillar_up(bot, feet_y(bot) + 3).await;
                        bot.set_control_state("sneak", false);
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
                bot.activate_item().await.ok();
                bot.wait_ticks(8).await.ok();
                if count_items(bot, &filled_bucket_name) > 0 {
                    cast_debug(&format!("fill {fluid}: OK (round {round})"));
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

// ── cast one obsidian block ───────────────────────────────────────────────────

/// Cast one obsidian block at `pos`: enclosed cup + lava, sealed bowl + water above.
async fn cast_obsidian_at(
    bot: &mut Bot<'_>,
    pos: (i32, i32, i32),
    base_y: i32,
    lava_pool: Option<(i32, i32, i32)>,
) -> bool {
    if name_at(bot, pos.0, pos.1, pos.2) == "obsidian" {
        return true;
    }
    let stand_z = pos.2 + 1;
    let above = (pos.0, pos.1 + 1, pos.2);
    {
        let p = bot.entity.position;
        cast_debug(&format!("cast {pos:?} ENTER bot=({:.1},{:.1},{:.1}) hp={:.0}", p.x, p.y, p.z, bot.health));
    }
    // WATER SWEEP before positioning. The PREVIOUS bottom-row block's water pour, if not fully
    // reclaimed, is a SOURCE whose flow both floods this cup AND physically SHOVES the bot off the
    // frame plane while it tries to stand (`!! underwater — surfacing`, then `POS FAIL off=3-5`, the
    // bot drifting to z+4 — the wall that pinned block 2). Scoop every water source around the frame
    // ONCE up front (flowing water vanishes permanently once its source is picked up), so the bot can
    // position AND the cup stays air. Needs an empty bucket; a harmless no-op if there's none / no water.
    {
        let water_near = (-2..=4)
            .any(|dx| (-2..=4).any(|dz| [0_i32, 1, -1, 2].iter().any(|&dy| name_at(bot, pos.0 + dx, pos.1 + dy, pos.2 + dz).contains("water"))));
        // DEADLOCK BREAKER: scooping the flood needs an EMPTY bucket. If the bot has none (all held
        // as lava+water — the bottom-row deadlock: 2 lava + 1 water + 0 empty), pour the water bucket
        // out to the SOUTH (away from the frame, never into the cup) to FREE an empty; the sweep then
        // scoops the flood SOURCE back into a bucket, restoring the water bucket and clearing the cup.
        if water_near && count_items(bot, "bucket") == 0 && count_items(bot, "water_bucket") >= 1 {
            select_item(bot, "water_bucket").await.ok();
            bot.look_at(vec3(pos.0 as f64 + 0.5, pos.1 as f64, stand_z as f64 + 3.0));
            bot.wait_ticks(4).await.ok();
            bot.activate_item().await.ok(); // pour water south → empties the bucket
            bot.wait_ticks(6).await.ok();
        }
        if water_near && select_item(bot, "bucket").await.unwrap_or(false) {
            for _ in 0..4 {
                let mut waters = Vec::new();
                for dy in [0_i32, 1, -1, 2] {
                    for dx in -2..=4 {
                        for dz in -2..=4 {
                            let w = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
                            if name_at(bot, w.0, w.1, w.2).contains("water") {
                                waters.push(w);
                            }
                        }
                    }
                }
                if waters.is_empty() {
                    break;
                }
                for w in waters {
                    reliable_use(bot, vec3(w.0 as f64 + 0.5, w.1 as f64 + 0.5, w.2 as f64 + 0.5)).await;
                }
                bot.wait_ticks(4).await.ok();
            }
            cast_debug(&format!("cast {pos:?}: pre-position water sweep → {}", name_at(bot, pos.0, pos.1, pos.2)));
        }
    }
    for _attempt in 0..5 {
        // DEAD (hp<=0): a corpse can't place cup walls / bowl pillar — every placement FAILS
        // ("cup-wall FAIL" / "bowl pillar FAIL") and the block loops a0-a4 → "cast stalled at
        // 0/10 obsidian" (013 hp=0 at ENTER). Respawn + abort so the step re-derives; the kit
        // (keep_inventory) and any placed obsidian persist, so the frame resumes alive.
        if bot.health <= 0.0 {
            cast_debug("cast: DEAD — respawning (a corpse can't place obsidian)");
            respawn_at_frame(bot).await;
            bot.set_control_state("sneak", false);
            return false;
        }
        // 0. Reconcile buckets. Casting needs ONE empty/lava bucket (to carry lava) and
        //    ONE water bucket. The racy reclaim can scoop the placed water while the bot
        //    still holds its original water bucket, leaving it with 2 water + 0 empty —
        //    from which it can never scoop lava again (the deadlock that strands the bot
        //    re-running fill_lava with empty=0 forever). Keep at most ONE water bucket:
        //    dump the extras (pour south, away from the cup) to free buckets for lava.
        while count_items(bot, "water_bucket") > 1 {
            select_item(bot, "water_bucket").await.ok();
            bot.look_at(vec3(pos.0 as f64 + 0.5, pos.1 as f64, stand_z as f64 + 3.0));
            bot.wait_ticks(4).await.ok();
            let before = count_items(bot, "water_bucket");
            bot.activate_item().await.ok(); // pour water out -> empty bucket
            bot.wait_ticks(6).await.ok();
            if count_items(bot, "water_bucket") >= before {
                break; // didn't pour (no aim/space) — avoid an infinite loop
            }
        }
        // Descend to BASE level before refilling lava. On an upper/top-row block the bot ENTERs
        // high (up on the previous block's pour-pillar); refilling from there drives fill_bucket
        // DOWN into the lava sea to reach the only source (at base level) and it burns to death —
        // the deep-sea 9/10 wall: casting the top row it logged "FOOTING on lava"/"below=lava" and
        // died at y64 in the sea. Get to base FIRST so the scoop is from a safe stand beside the
        // sea; step 2 then pillars up WITH the lava bucket to cast. (On solid ground base==lava
        // level so this is a no-op; it only matters over an open sea where the frame rises above.)
        if count_items(bot, "lava_bucket") < 1 && feet_y(bot) > base_y {
            descend_to_y(bot, base_y).await;
        }
        // 1. Refill lava. Scoop the NEAREST lava FIRST: the frame anchors ~4 blocks off the
        //    pool's edge, so the edge is right here — this keeps the bot AT the frame. Only
        //    walk to the remembered far pool as a FALLBACK. Going to the far pool first
        //    stranded the bot on the wrong side of a wide lava lake, unable to path back
        //    across to the frame (cast block 2 at (4541) from (4524), 17 away, lava between).
        // Skip the refill (and its costly/failing height trip) when the cup ALREADY holds lava —
        // this attempt only needs to get the WATER right, not more lava. Without this, the last
        // top block re-triggered `fill lava: all rounds failed` every retry and stalled at 9/10.
        if count_items(bot, "lava_bucket") < 1 && !name_at(bot, pos.0, pos.1, pos.2).contains("lava") {
            // Refill from the KNOWN-SAFE scoop stand (where prepare scooped and survived), NOT by
            // letting fill_bucket re-find a stand from the chamber side — that pillared beside the
            // pool and burned the bot to death (EARLY BAIL hp=0). Walk back to the exact safe spot
            // and scoop; retry a few times. Only if there's no stored stand fall back to the pool.
            let safe = *SAFE_SCOOP_STAND.lock().unwrap();
            let target = (*FORCED_LAVA.lock().unwrap()).or(lava_pool);
            bot.movement.blocks_cant_break.clear();
            // BATCH-scoop up to 2 lava per trip so the bot casts ~2 blocks before returning —
            // halving the slow chamber↔pool round-trips (the dominant per-block cost that made a
            // full 10-block frame exceed budget: ~2.5min/block → only ~1 block cast in 5min).
            for _ in 0..6 {
                if count_items(bot, "lava_bucket") >= 2 || count_items(bot, "bucket") < 1 {
                    break;
                }
                let before = count_items(bot, "lava_bucket");
                // Try the known-safe stand first.
                if let Some(s) = safe {
                    let _ = bot.goto_near(s.0.floor() as i32, s.1.floor() as i32, s.2.floor() as i32, 1.5).await;
                    walk_to_xz(bot, s.0, s.2, 0.4, 40).await;
                    fill_bucket(bot, "lava").await;
                }
                // If that yielded nothing (e.g. the safe stand is unreachable from the frame TOP at
                // y77 — the top-row refill-from-height failure), fall through to the ACTUAL nearest
                // exposed lava SOURCE and descend to ITS level, then scoop there. The stored POI y
                // can be stale (POI y=-47 but the real scoopable source at -55), so descending to
                // pool.1+1 was a NO-OP that left the bot up on the frame ~5 blocks from the lava,
                // scooping air (hdist=5.3, `lava true->lava`) — the real-terrain block-2 refill wall.
                if count_items(bot, "lava_bucket") <= before {
                    let nearest_src = bot
                        .find_exposed_blocks("lava", 20, 400)
                        .into_iter()
                        .find(|&(x, y, z)| is_fluid_source(bot, x, y, z, "lava"))
                        .or(target.map(|t| (t.0, t.1, t.2)));
                    if let Some((lx, ly, lz)) = nearest_src {
                        let _ = bot.goto_near(lx, ly + 1, lz, 2.0).await;
                        descend_to_y(bot, ly + 1).await;
                        // Close the last gap horizontally so the scoop raycast can actually reach the
                        // source (goto_near stops ~1.5 out; from 5 away the aim misses).
                        walk_to_xz(bot, lx as f64 + 0.5, lz as f64 + 0.5, 0.8, 40).await;
                        fill_bucket(bot, "lava").await;
                    }
                }
            }
            if count_items(bot, "lava_bucket") < 1 {
                return false;
            }
            // DE-FIRE + HEAL, but ONLY when actually hurt / on fire. Scooping often nicks the bot,
            // and those accumulate over ~10 refills → death mid-build. BUT retreating every time
            // (even healthy) moves the bot off the frame line and perturbs the razor-thin pour
            // stance for the next block (block 2 then missed its cup). So retreat only when needed.
            if bot.health < 16.0 || raw_lava_near(bot, 2) {
                let _ = bot.goto_near(pos.0, base_y, stand_z + 3, 1.5).await;
                // At the OPEN lava band (natural terrain, no fire-safe chamber) the horizontal
                // retreat can't clear the lava — sources ring the tunnel, so raw_lava_near stays
                // true, the water-extinguish below is skipped, and the bot burns in place (hp=0
                // ABORT loop casting block 2+). PILLAR UP onto cobble to rise OUT of the lava's
                // fire radius so it can then douse + heal, and come back down only to scoop.
                if raw_lava_near(bot, 3) {
                    let escape_y = feet_y(bot) + 4;
                    let _ = pillar_up(bot, escape_y).await;
                }
                // EXTINGUISH with water: after a lava nick the bot stays ON FIRE ~7s and burns to 0
                // faster than eating heals (that's how it died at block ~4). Placing a water bucket
                // at the feet puts the fire out INSTANTLY. Only when clear of lava (water+lava would
                // react); then scoop the water back so the cast keeps its bucket.
                if bot.health < 18.0 && count_items(bot, "water_bucket") >= 1 && !raw_lava_near(bot, 3) {
                    let p = bot.entity.position;
                    let (fx, fy, fz) = (p.x.floor() as i32, feet_y(bot), p.z.floor() as i32);
                    if solid_at(bot, fx, fy - 1, fz) {
                        select_item(bot, "water_bucket").await.ok();
                        bot.look_at(vec3(fx as f64 + 0.5, (fy - 1) as f64 + 0.9, fz as f64 + 0.5));
                        bot.wait_ticks(2).await.ok();
                        let _ = bot.place_block(fx, fy - 1, fz, Face::Top).await;
                        bot.wait_ticks(8).await.ok(); // fire out
                        select_item(bot, "bucket").await.ok();
                        for (dx, dz) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                            if count_items(bot, "water_bucket") >= 1 {
                                break;
                            }
                            reliable_use(bot, vec3((fx + dx) as f64 + 0.5, fy as f64 + 0.5, (fz + dz) as f64 + 0.5)).await;
                        }
                    }
                }
                for _ in 0..10 {
                    if bot.health >= 18.0 && !raw_lava_near(bot, 3) {
                        break;
                    }
                    eat_if_hurt(bot).await;
                    bot.wait_ticks(15).await.ok();
                }
            }
        }
        if count_items(bot, "water_bucket") < 1 && !fill_bucket(bot, "water").await {
            return false;
        }

        // 2. Position at the floor spot in front of the cup. The previous block left
        //    the bot up on its pour-pillar, so DESCEND to base level first (dig the
        //    pillar away), THEN navigate to the stand spot and re-descend. Retry a few
        //    times before giving up — the cluttered frame makes a single try unreliable.
        bot.set_control_state("sneak", false);
        let dist_from_stand = |bot: &Bot| {
            (bot.entity.position.x - (pos.0 as f64 + 0.5)).abs()
                + (bot.entity.position.z - (stand_z as f64 + 0.5)).abs()
        };
        // TOP-ROW handling: the highest blocks (y = base_y+4) can't reposition down to base_y — the
        // built columns block the descent — and have no footing at feet pos.1+1 (their +Z stand wall
        // is itself a cup wall built during the cast). So build the +Z stand wall + a block behind it
        // from up here, pillar onto it, and SKIP the base-level reposition (which POS-FAILed at 8/10).
        let top_row = pos.1 >= base_y + 4;
        if top_row {
            bot.set_control_state("sneak", true);
            // A 1-wide perch on the +Z wall made the stand HEIGHT vary run-to-run (feet 78/79/80)
            // because the bot drifted in x off the single block and pillar_up re-stacked at the
            // drifted cell. Build a WIDE FLAT platform at the bowl-stand level (y = pos.1+1) that
            // spans x∈[pos.0-1, pos.0+1] × z∈[stand_z, stand_z+1] so ANY small drift still lands
            // the bot on a flat surface at feet = pos.1+2. NEVER place at z=pos.2 (that column is
            // the water bowl above the cup). Plus footing-below + cup-level +Z for containment.
            let plat_y = pos.1 + 1;
            for dx in -1..=1 {
                for dz in 0..=1 {
                    let c = (pos.0 + dx, plat_y, stand_z + dz);
                    if !solid_at(bot, c.0, c.1, c.2) {
                        ensure_solid(bot, c, 0).await;
                    }
                }
            }
            for w in [(pos.0, pos.1 - 1, stand_z), (pos.0, pos.1, stand_z)] {
                if !solid_at(bot, w.0, w.1, w.2) {
                    ensure_solid(bot, w, 0).await;
                }
            }
            // Clear any debris ABOVE the platform on the stand columns so the bot can't perch a
            // block or two too high (feet 79/80). A leftover cobble from a prior block's pillar left
            // the bot stuck at feet=80 and it burned a whole frame-pass failing to dig down to 78.
            for cz in [stand_z, stand_z + 1] {
                for cy in (pos.1 + 2)..=(pos.1 + 6) {
                    let n = name_at(bot, pos.0, cy, cz);
                    if is_solid(&n) && n != "obsidian" {
                        dig_at(bot, pos.0, cy, cz).await;
                    }
                }
            }
            let _ = bot.goto_near(pos.0, pos.1 + 2, stand_z, 1.0).await;
            if feet_y(bot) < pos.1 + 2 {
                pillar_up(bot, pos.1 + 2).await;
            }
            // Pin x/z on the platform centre, then force feet EXACTLY to pos.1+2 by digging any
            // overshoot the bot is standing on (never obsidian/lava). Wide platform below means it
            // always drops onto flat ground at pos.1+1 → feet pos.1+2.
            for _ in 0..6 {
                walk_to_xz(bot, pos.0 as f64 + 0.5, stand_z as f64 + 0.5, 0.12, 40).await;
                if feet_y(bot) > pos.1 + 2 {
                    let (fx, fz) = (bot.entity.position.x.floor() as i32, bot.entity.position.z.floor() as i32);
                    let below = name_at(bot, fx, feet_y(bot) - 1, fz);
                    if is_solid(&below) && below != "obsidian" && !is_lava(&below) {
                        dig_at(bot, fx, feet_y(bot) - 1, fz).await;
                        bot.wait_ticks(6).await.ok();
                    }
                }
                if dist_from_stand(bot) <= 0.6 && feet_y(bot) <= pos.1 + 2 {
                    break;
                }
            }
            // Gate on CENTERING (drift off x/z is what actually breaks the cup+pour), but tolerate the
            // stand being ONE block too high (feet pos.1+3). The lava pour and water steps below each
            // RE-DESCEND to the exact level (feet pos.1+1) with their own dig-down, so an exact feet
            // match here is redundant — and demanding it stranded the last top block forever at feet 79
            // (the debris-perch the feet-enforce couldn't dig, e.g. standing on a placed cup wall).
            // Allow feet up to pos.1+4 (was +3): the pour/water steps re-descend to the exact
            // level with their own dig-down, so height here is only a sanity bound — and +3
            // rejected the ONLY centred attempt (011 stuck at 7/10, top row: off=0.15 but
            // feet=pos.1+4, POS FAIL every pass). Centring (off) is what actually matters.
            if dist_from_stand(bot) > 1.0 || feet_y(bot) > pos.1 + 4 {
                let p = bot.entity.position;
                cast_debug(&format!("cast {pos:?} a{_attempt}: TOP-ROW POS FAIL off={:.2} feet={} want<={} bot=({:.1},{:.1},{:.1})", dist_from_stand(bot), feet_y(bot), pos.1 + 4, p.x, p.y, p.z));
                bot.set_control_state("sneak", false);
                continue;
            }
        } else {
            // Stand ON the +Z cup-wall level (feet base_y+1), not at base_y. At base_y the bot is at
            // the CUP level, and a +Z cup wall built by a prior attempt (at pos.1,stand_z) BLOCKS it
            // from reaching the stand — it gets stuck at z=pos.2+0.7, off≈1.4 → POS FAIL, oscillating.
            // Standing one higher (on the wall) clears the blockage; it's also where the lava pour
            // wants the feet (pos.1+1). On the FIRST cell (no wall yet) the bot just falls to base_y,
            // which is fine — the accept check allows feet up to base_y+1.
            let stand_feet = base_y + 1;
            descend_to_y(bot, base_y).await; // off any previous pour-pillar first
            for try_pos in 0..3 {
                let _ = bot.goto_near(pos.0, stand_feet, stand_z, 1.0).await;
                if feet_y(bot) < stand_feet {
                    pillar_up(bot, stand_feet).await;
                }
                // Drop any overshoot so the feet land exactly on the +Z wall (never obsidian/lava).
                for _ in 0..3 {
                    if feet_y(bot) <= stand_feet {
                        break;
                    }
                    let (fx, fz) = (bot.entity.position.x.floor() as i32, bot.entity.position.z.floor() as i32);
                    let below = name_at(bot, fx, feet_y(bot) - 1, fz);
                    if is_solid(&below) && below != "obsidian" && !is_lava(&below) {
                        dig_at(bot, fx, feet_y(bot) - 1, fz).await;
                        bot.wait_ticks(6).await.ok();
                    } else {
                        break;
                    }
                }
                // EXCAVATE the stand + approach lane. On natural deep terrain the chamber clear
                // doesn't always cover THIS block's stand — block 2's stand column (pos.0, ·, stand_z)
                // can be solid deepslate, so the bot reaches staging (stand_z+3) but can't walk IN to
                // the stand (z stuck ~4 off → POS FAIL off=3.77 forever, frame frozen at 1/10). Dig the
                // stand cell + the approach cells behind it (feet+head), never obsidian/lava, so the
                // walk-in has a clear path. Progressive: closer cells clear as the bot advances.
                for dz in 0..=3 {
                    let cz = stand_z + dz;
                    // FLOOR first: lay a solid non-lava floor to walk in on. On the natural lava band
                    // the stand floor (stand_feet-1) is often a gap or lava, so the bot can't cross to
                    // the stand and stalls ~1.7 off at the last solid cell (POS FAIL, frame frozen at
                    // 1/10). Cobble it (also caps any lava there). Never overwrite obsidian.
                    let floor = name_at(bot, pos.0, stand_feet - 1, cz);
                    if (is_air(&floor) || is_lava(&floor)) && count_items(bot, &build_block(bot)) > 0 {
                        place_cobble(bot, (pos.0, stand_feet - 1, cz)).await;
                    }
                    // LANE: clear feet+head so the walk-in has headroom (never obsidian/lava).
                    for dy in [stand_feet, stand_feet + 1] {
                        let b = name_at(bot, pos.0, dy, cz);
                        if is_solid(&b) && b != "obsidian" && !is_lava(&b) {
                            dig_at(bot, pos.0, dy, cz).await;
                        }
                    }
                }
                walk_to_xz(bot, pos.0 as f64 + 0.5, stand_z as f64 + 0.5, 0.3, 60).await;
                if dist_from_stand(bot) <= 1.2 && feet_y(bot) <= stand_feet {
                    break;
                }
                // Stage from a clear spot behind the working line, then re-approach.
                let _ = bot.goto_near(pos.0, stand_feet, stand_z + 3, 1.0).await;
                if try_pos == 2 {
                    let p = bot.entity.position;
                    cast_debug(&format!("cast {pos:?} a{_attempt}: POS FAIL off={:.2} feet={} bot=({:.1},{:.1},{:.1})", dist_from_stand(bot), feet_y(bot), p.x, p.y, p.z));
                }
            }
        }
        // Coarse pre-gate relaxed 1.4→1.8: the deterministic cup-build (place_block, reach ~4.5) and
        // pour don't need a dead-centre stance, and the finer walk_to_xz + the 0.30 centering gate
        // below still govern the actual pour. The tight 1.4 aborted block 2 at off=1.74 even after the
        // floor+lane excavation got it close, freezing the frame at 1/10.
        if dist_from_stand(bot) > 1.8 {
            bot.set_control_state("sneak", false);
            return false;
        }
        if feet_y(bot) < pos.1 && !pillar_up(bot, pos.1).await {
            cast_debug(&format!("cast {pos:?} a{_attempt}: pillar1 FAIL feet={}", feet_y(bot)));
            continue;
        }
        cast_debug(&format!(
            "cast {pos:?} a{_attempt}: positioned off={:.2} feet={} lava_b={} water_b={}",
            dist_from_stand(bot),
            feet_y(bot),
            count_items(bot, "lava_bucket"),
            count_items(bot, "water_bucket")
        ));

        // 2b. CLEAN UP any stray lava left near the cup by a previous attempt's missed
        //     pour (it floods the area and blocks rebuilding the cup / the water). Let
        //     it settle, then scoop every lava source in a region around the target with
        //     the empty bucket — sources pick up and dissolve the flowing parts. Skip
        //     the lava already correctly in the cup.
        let stray_lava: bool = (-1..=1).any(|dx| {
            (0..=2).any(|dy| {
                (-1..=2).any(|dz| {
                    !(dx == 0 && dy == 0 && dz == 0)
                        && name_at(bot, pos.0 + dx, pos.1 + dy, pos.2 + dz).contains("lava")
                })
            })
        });
        if stray_lava {
            bot.wait_ticks(20).await.ok();
            select_item(bot, "bucket").await.ok();
            for _ in 0..10 {
                let mut target = None;
                'find: for dy in [1, 2, 0] {
                    for dx in -1..=1 {
                        for dz in -1..=2 {
                            if dx == 0 && dy == 0 && dz == 0 {
                                continue; // leave the cup's own lava
                            }
                            let l = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
                            if name_at(bot, l.0, l.1, l.2).contains("lava") {
                                target = Some(l);
                                break 'find;
                            }
                        }
                    }
                }
                let Some(l) = target else { break };
                reliable_use(bot, vec3(l.0 as f64 + 0.5, l.1 as f64 + 0.5, l.2 as f64 + 0.5)).await;
            }
            cast_debug(&format!("cast {pos:?} a{_attempt}: cleaned stray lava"));
        }

        // 3. Cup walls (E, W, N, below). The +Z wall is the pillar we stand on.
        let mut cup_ok = true;
        for s in [
            (pos.0 + 1, pos.1, pos.2),
            (pos.0 - 1, pos.1, pos.2),
            (pos.0, pos.1, pos.2 - 1),
            (pos.0, pos.1 - 1, pos.2),
        ] {
            if !solid_at(bot, s.0, s.1, s.2) && !ensure_solid(bot, s, 0).await {
                cup_ok = false;
                cast_debug(&format!("cast {pos:?} a{_attempt}: cup-wall FAIL at {s:?} (feet={})", feet_y(bot)));
                break;
            }
        }
        if !cup_ok {
            continue;
        }
        // Water-bowl walls one level up (E, W, N). +Z bowl wall comes from the next pillar.
        ensure_solid(bot, (pos.0 + 1, pos.1 + 1, pos.2), 0).await;
        ensure_solid(bot, (pos.0 - 1, pos.1 + 1, pos.2), 0).await;
        ensure_solid(bot, (pos.0, pos.1 + 1, pos.2 - 1), 0).await;

        walk_to_xz(bot, pos.0 as f64 + 0.5, stand_z as f64 + 0.5, 0.25, 30).await;
        // Pillar one more to pour height (feet = pos.y+1); drops the +Z cup wall.
        if feet_y(bot) < pos.1 + 1 && !pillar_up(bot, pos.1 + 1).await {
            continue;
        }
        if !is_air(&name_at(bot, pos.0, pos.1, pos.2)) {
            dig_at(bot, pos.0, pos.1, pos.2).await;
        }
        if !is_air(&name_at(bot, above.0, above.1, above.2)) {
            dig_at(bot, above.0, above.1, above.2).await;
        }
        walk_to_xz(bot, pos.0 as f64 + 0.5, stand_z as f64 + 0.5, 0.3, 30).await;

        // Cup gate: all 5 walls solid (E,W,+Z,-Z,below) before pouring lava.
        let walls = [
            (pos.0 + 1, pos.1, pos.2),
            (pos.0 - 1, pos.1, pos.2),
            (pos.0, pos.1, pos.2 + 1),
            (pos.0, pos.1, pos.2 - 1),
            (pos.0, pos.1 - 1, pos.2),
        ];
        for w in walls {
            if !solid_at(bot, w.0, w.1, w.2) {
                ensure_solid(bot, w, 0).await;
            }
        }
        walk_to_xz(bot, pos.0 as f64 + 0.5, stand_z as f64 + 0.5, 0.3, 30).await;
        let cup: String = walls.iter().map(|w| if solid_at(bot, w.0, w.1, w.2) { 'S' } else { '_' }).collect();
        cast_debug(&format!("cast {pos:?} a{_attempt}: cup={cup} feet={}", feet_y(bot)));
        if walls.iter().any(|w| !solid_at(bot, w.0, w.1, w.2)) {
            continue; // never pour lava into a leaky cup
        }

        // 4. Pour LAVA into the cup. A bucket pour is a raycast (use_item), so the aim
        //    matters: the straight-down aim lands it in the cup in the open, but in a
        //    cluttered frame (the previous block's walls beside this one) it can deflect
        //    and miss. A pour empties the bucket whether or not it landed right, so we
        //    can only pour ONCE per attempt — vary the aim by attempt number, and the
        //    outer 3-attempt loop refills the lava between tries.
        // CRITICAL: pour from EXACTLY feet pos.y+1. Coming off the previous block's
        // pillar the bot is often a block too HIGH (feet pos.y+2), and from there the
        // steep down-ray hits the +Z cup wall's top and drops the lava on the bot's own
        // block. Descend to the right level (digging only the throwaway scaffold above
        // the cup, never the cup walls) before pouring.
        // Drop to EXACTLY feet pos.y+1 — the +Z cup-wall top. The bot stands on the +Z
        // wall at (pos.0, pos.1, pos.2+1); when a previous block left debris one block up
        // (at pos.y+1 on that same cell) the bot perches on it at feet pos.y+2, and from
        // there the down-ray overshoots the cup and the pour misses (cup stays air). The
        // generic footprint descend chases the bot's floating cell and misses this exact
        // column, so clear it explicitly — everything ABOVE the +Z wall on the stand cell,
        // never the wall itself (pos.1) or the cup walls (z = pos.2-1/pos.2).
        for dy in [3, 2, 1] {
            let c = (pos.0, pos.1 + dy, pos.2 + 1);
            let n = name_at(bot, c.0, c.1, c.2);
            if is_solid(&n) && n != "obsidian" {
                dig_at(bot, c.0, c.1, c.2).await;
                bot.wait_ticks(4).await.ok();
            }
        }
        descend_to_y(bot, pos.1 + 1).await;
        if feet_y(bot) < pos.1 + 1 {
            pillar_up(bot, pos.1 + 1).await;
        }
        // Precisely re-center on the pour cell and SETTLE before pouring — a missed
        // pour floods unscoopable flowing lava into the water-target block, so landing
        // it first try matters. Sneak so the tight walk can't slip off the 1-wide pillar.
        // The pour geometry is razor-thin — a ~2° look difference (i.e. the bot a few
        // tenths off-centre) sends the lava onto the +Z wall instead of into the cup,
        // and a miss damages the bot + floods. So centre HARD and, if still not dead-on,
        // skip this attempt rather than pour a likely miss.
        // Stand at the NORTH edge of the +Z wall (close to the cup), not its centre. The
        // pour ray drops through the bot's own stand cell before reaching the cup; from
        // the cell centre (z = stand_z+0.5) it only clears the +Z wall's top face by
        // ~0.4 block, so a 0.01 jitter makes it clip the wall top and dump the lava at the
        // bot's feet (cup stays air — exactly how the 2nd block kept failing). Sitting at
        // the cup-side edge (+0.3) lets the ray exit this cell almost immediately, high up,
        // clearing the wall top by ~0.8 block — robust to jitter.
        // LAND on the +Z wall at feet EXACTLY pos.1+1 first. Coming off the adjacent block the bot
        // perches 1-2 blocks too high on debris (feet 74-75, even on the WRONG cell x-1), and from
        // there BOTH the deterministic place and the aim pour miss the cup (the block-2 wall). Clear
        // the stand column + the row behind, walk onto the +Z wall, and dig straight down under the
        // feet until level — never digging the cup walls (pos.1) or obsidian.
        for (cx, cz) in [(pos.0, stand_z), (pos.0, stand_z + 1)] {
            for dy in [3, 2, 1] {
                let n = name_at(bot, cx, pos.1 + dy, cz);
                if is_solid(&n) && n != "obsidian" {
                    dig_at(bot, cx, pos.1 + dy, cz).await;
                }
            }
        }
        walk_to_xz(bot, pos.0 as f64 + 0.5, stand_z as f64 + 0.5, 0.25, 40).await;
        for _ in 0..4 {
            if feet_y(bot) <= pos.1 + 1 {
                break;
            }
            let (fx, fz) = (bot.entity.position.x.floor() as i32, bot.entity.position.z.floor() as i32);
            let below = name_at(bot, fx, feet_y(bot) - 1, fz);
            if is_solid(&below) && below != "obsidian" && !is_lava(&below) {
                dig_at(bot, fx, feet_y(bot) - 1, fz).await;
                bot.wait_ticks(6).await.ok();
            } else {
                break;
            }
        }
        let pour_z = stand_z as f64 + 0.3;
        bot.set_control_state("sneak", true);
        for _ in 0..3 {
            walk_to_xz(bot, pos.0 as f64 + 0.5, pour_z, 0.06, 60).await;
            let p = bot.entity.position;
            if (p.x - (pos.0 as f64 + 0.5)).abs() < 0.12 && (p.z - pour_z).abs() < 0.12 {
                break;
            }
        }
        bot.wait_ticks(4).await.ok();
        let p = bot.entity.position;
        // Relaxed from 0.15/0.18: block 2 (x-adjacent to fresh block-1 obsidian) settles ~0.16 off
        // because walk_to_xz can't push through block-1's cup wall to the 0.06 target — the OLD tight
        // gate rejected it and stalled the frame at 1/10 forever (centered=false, natural terrain).
        // Safe to widen now: the pour is DETERMINISTIC (place_block against the cup wall lands lava/
        // water IN the cup regardless of exact stance), so the old off-centre flood/miss risk that
        // motivated the tight gate no longer applies; the wall-seal + centre-clear guards still hold.
        let centered = (p.x - (pos.0 as f64 + 0.5)).abs() < 0.30 && (p.z - pour_z).abs() < 0.34;
        cast_debug(&format!(
            "cast {pos:?} a{_attempt}: pre-lava feet={} bot=({:.2},{:.2}) centered={centered}",
            feet_y(bot), p.x, p.z
        ));
        if !centered {
            continue; // don't pour from off-centre — it'll miss, damage us, and flood
        }
        // CLEAR WATER flooding the cup — this must run FIRST (before the centre dig below), and it
        // must scoop the water SOURCE, not the cup cell. The PREVIOUS bottom-row block's water pour
        // spreads into this x-adjacent cup (bottom cups share an edge); the cup reads water so the
        // deterministic lava place (which needs is_air(cup)) drops NOTHING (`lava_at=[]`) → frame
        // stalls at 1/10. Digging the cup is futile — flowing water REFLOWS from its source between
        // the dig and the place (why the earlier cup==water check saw a momentary air and skipped).
        // So: scan a radius for ANY water source and scoop it (flowing water vanishes the instant
        // its source is picked up), gated on there being water NEAR the cup, not IN it right now.
        {
            let water_near = (-3..=3).any(|dx| {
                (-3..=3).any(|dz| {
                    [0_i32, 1, 2, -1].iter().any(|&dy| name_at(bot, pos.0 + dx, pos.1 + dy, pos.2 + dz).contains("water"))
                })
            });
            if water_near && select_item(bot, "bucket").await.unwrap_or(false) {
                for _ in 0..4 {
                    let mut waters = Vec::new();
                    for dy in [0_i32, 1, 2, -1] {
                        for dx in -3..=3 {
                            for dz in -3..=3 {
                                let w = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
                                if name_at(bot, w.0, w.1, w.2).contains("water") {
                                    waters.push(w);
                                }
                            }
                        }
                    }
                    if waters.is_empty() {
                        break;
                    }
                    for w in waters {
                        reliable_use(bot, vec3(w.0 as f64 + 0.5, w.1 as f64 + 0.5, w.2 as f64 + 0.5)).await;
                    }
                    bot.wait_ticks(4).await.ok(); // let flow recede after scooping a source
                }
                cast_debug(&format!("cast {pos:?} a{_attempt}: cleared cup water → {}", name_at(bot, pos.0, pos.1, pos.2)));
            }
        }
        // Cup CENTER must be hollow before pouring. The two top-middle frame blocks are
        // x-adjacent, so casting one builds its E/W cup wall INTO the other's centre cell
        // (ensure_solid), and the single early dig(pos) upstream doesn't always take on the
        // tall top-row pillar. Pouring lava into a solid centre is a guaranteed no-op — the
        // exact top-row failure (log: "after_lava cup_block=cobblestone"). The wall gate above
        // checks the 5 walls but NOT the centre. Re-clear it here (dead-centred + settled),
        // and skip the attempt rather than waste the bucket + flood if it won't clear.
        {
            let mut center = name_at(bot, pos.0, pos.1, pos.2);
            if !is_air(&center) && !center.contains("lava") {
                for _ in 0..2 {
                    dig_at(bot, pos.0, pos.1, pos.2).await;
                    bot.wait_ticks(4).await.ok();
                    center = name_at(bot, pos.0, pos.1, pos.2);
                    if is_air(&center) {
                        break;
                    }
                }
                if !is_air(&name_at(bot, pos.0, pos.1, pos.2)) {
                    cast_debug(&format!("cast {pos:?} a{_attempt}: cup centre still solid ({center}) — skip pour"));
                    continue;
                }
            }
        }
        select_item(bot, "lava_bucket").await.ok();
        // Place lava DETERMINISTICALLY against the -Z cup wall's +Z face, EXACTLY like the water
        // bowl does below — lava lands as a source in the cup cell (pos) regardless of aim. The old
        // aim-down raycast pour missed the cup on the block ADJACENT to fresh obsidian (`cup_block=
        // air`) and stalled the whole frame; the water placement proves place_block works for
        // buckets (use_item_on), so the "buckets can't place" claim was wrong for this geometry.
        let lava_ref = (pos.0, pos.1, pos.2 - 1); // the -Z cup wall
        if solid_at(bot, lava_ref.0, lava_ref.1, lava_ref.2) && is_air(&name_at(bot, pos.0, pos.1, pos.2)) {
            // Aim INTO the cup interior (its centre, near the floor) — NOT at the -Z wall face. From
            // feet pos.1+1 the old aim at (pos.2, pos.1+0.5) made too steep a down-ray that the LOOK-RAY
            // (buckets place along it) intercepted on the +Z cup-wall TOP, dropping lava at the bot's
            // feet cell (pos.1+1, stand_z) — `lava_at=[(x,pos.1+1,stand_z)]`, cup stayed air. Aiming at
            // the cup's own centre-floor clears the +Z wall lip and lands the source in the cup.
            bot.look_at(vec3(pos.0 as f64 + 0.5, pos.1 as f64 + 0.2, pos.2 as f64 + 0.5));
            bot.wait_ticks(3).await.ok();
            let _ = bot.place_block(lava_ref.0, lava_ref.1, lava_ref.2, Face::South).await;
            bot.wait_ticks(6).await.ok();
        }
        // Fallback to the aim pour only if the cup is still air (deterministic place blocked/failed).
        // Every variant aims DOWN INTO the cup cell (z=pos.2+0.5 centre, low y) so the ray clears the
        // +Z wall lip and drops the source in the cup rather than on the bot's stand.
        if is_air(&name_at(bot, pos.0, pos.1, pos.2)) {
            let lava_aim = match _attempt {
                0 => vec3(pos.0 as f64 + 0.5, pos.1 as f64 + 0.2, pos.2 as f64 + 0.5),
                1 => vec3(pos.0 as f64 + 0.5, pos.1 as f64 + 0.1, pos.2 as f64 + 0.4),
                2 => vec3(pos.0 as f64 + 0.5, pos.1 as f64 + 0.3, pos.2 as f64 + 0.5),
                3 => vec3(pos.0 as f64 + 0.5, pos.1 as f64 + 0.1, pos.2 as f64 + 0.6),
                _ => vec3(pos.0 as f64 + 0.5, pos.1 as f64 + 0.2, pos.2 as f64 + 0.3),
            };
            reliable_use(bot, lava_aim).await;
            bot.wait_ticks(8).await.ok();
        }
        {
            // Where did the lava actually land? (pour-miss diagnosis, mirrors water_at) — a bucket
            // places along the LOOK-RAY, so a miss lands it somewhere other than the cup (pos).
            let mut llocs = Vec::new();
            for dx in -1..=1 {
                for dy in -1..=2 {
                    for dz in -2..=2 {
                        if name_at(bot, pos.0 + dx, pos.1 + dy, pos.2 + dz).contains("lava") {
                            llocs.push((pos.0 + dx, pos.1 + dy, pos.2 + dz));
                        }
                    }
                }
            }
            let p = bot.entity.position;
            cast_debug(&format!("cast {pos:?} a{_attempt}: lava_at={llocs:?} eye=({:.2},{:.2},{:.2})", p.x, p.y + 1.62, p.z));
        }
        cast_debug(&format!(
            "cast {pos:?} a{_attempt}: after_lava cup_block={} lava_lvl={:?}",
            name_at(bot, pos.0, pos.1, pos.2),
            bot.block_at(pos.0, pos.1, pos.2).and_then(|b| b.properties.get("level").cloned())
        ));
        // If the lava missed the cup, this attempt is wasted AND the misplaced lava is
        // likely at the bot's own feet (the +Z wall top) — ESCAPE it (sprint-jump back
        // south) before it burns us, then SCOOP the misplaced source so it doesn't flood
        // the work area across retries (an uncleaned flood is what eventually traps the
        // bot and deadlocks its buckets). The escape recovers the empty bucket; scooping
        // the stray source refills it to lava, ready for the next attempt.
        if !name_at(bot, pos.0, pos.1, pos.2).contains("lava") {
            bot.set_control_state("sneak", false);
            // Escape the misplaced lava but STAY ON THE PLATFORM. The old sprint-jump SOUTH
            // (stand_z+4) overshot the 8x5 cast platform (only bz-2..bz+2) straight into the
            // open lava sea and caught fire — THE reason deep-sea casts stalled ~1-3/10 while
            // solid-ground (where south is solid) reached 10/10. Instead retreat WEST across the
            // platform's wide side (bx-2..bx+5) one VERIFIED-solid step at a time, so we move off
            // the misplaced-lava cup without ever stepping into the sea.
            for back in 1..=3 {
                let tx = pos.0 - back;
                if !solid_at(bot, tx, pos.1 - 1, stand_z) {
                    break; // platform edge — don't step into lava
                }
                let _ = bot.goto_near(tx, pos.1, stand_z, 0.6).await;
            }
            // Scoop every stray lava source around the cup top (not the pool) so the next
            // attempt starts from a clean, un-flooded site.
            bot.wait_ticks(20).await.ok();
            if count_items(bot, "bucket") > 0 {
                select_item(bot, "bucket").await.ok();
                for _ in 0..6 {
                    let mut stray = None;
                    'scan: for dy in [1, 2, 0] {
                        for dx in -1..=1 {
                            for dz in -1..=2 {
                                let l = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
                                if name_at(bot, l.0, l.1, l.2).contains("lava") {
                                    stray = Some(l);
                                    break 'scan;
                                }
                            }
                        }
                    }
                    let Some(l) = stray else { break };
                    reliable_use(bot, vec3(l.0 as f64 + 0.5, l.1 as f64 + 0.5, l.2 as f64 + 0.5)).await;
                    if count_items(bot, "lava_bucket") > 0 {
                        break; // recovered a lava bucket; the rest will re-flow/settle
                    }
                }
            }
            // NEUTRALIZE any remaining stray lava around the cup + stand lane with COBBLE. The
            // single empty bucket scoops only ONE source, but a stubborn miss can spawn several
            // (and refill both buckets), leaving a spreading pool on the stone the bot later steps
            // into and dies to. A block placed into a lava cell replaces it — no empty bucket
            // needed — so cap the stray. Skip the cup cell/floor (they're not lava on a miss) and
            // don't touch the lake (east, far from the frame). Cover the +Z stand/approach side.
            // dx -2..=1 (not +2): never cobble the lava LAKE, which sits ~2 east of the right
            // column — stray from a miss lands at the bot's feet and flows west/south onto the
            // stone, so the west + stand side is the danger zone.
            for dy in [0, 1, 2, 3] {
                for dx in -2..=1 {
                    for dz in -1..=4 {
                        let l = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
                        if l == pos {
                            continue; // never cap the cup itself — the next attempt pours lava here
                        }
                        // Cap stray LAVA *and* WATER with cobble. Water was the missing case: the
                        // adjacent done block's water pour floods THIS cup's neighbourhood and racy
                        // scooping keeps letting it reflow (`cleared→air` then `cup_block=water`),
                        // deadlocking the cell forever at 2/10. A block placed into the water SOURCE
                        // cell kills it deterministically (no reflow, no empty bucket needed); any
                        // interior cells capped here are dug back to air before the portal is lit.
                        let n = name_at(bot, l.0, l.1, l.2);
                        if n.contains("lava") || n.contains("water") {
                            place_cobble(bot, l).await;
                        }
                    }
                }
            }
            continue;
        }

        // 4b. A missed lava pour floods FLOWING lava into the cup's `above` block
        //     (pos.y+1) — where the water must go — and flowing lava can't be scooped.
        //     But the flood's SOURCE block (wherever the bucket dropped it) is a real
        //     source that IS scoopable, and removing it makes the whole flow vanish.
        //     Let it settle, then scoop every lava block in a small region around the
        //     cup's top until `above` is clear (the source picks up, refilling lava).
        if name_at(bot, above.0, above.1, above.2).contains("lava") {
            bot.wait_ticks(20).await.ok(); // let the flow settle so sources are stable
            select_item(bot, "bucket").await.ok();
            'clear: for _ in 0..8 {
                if !name_at(bot, above.0, above.1, above.2).contains("lava") {
                    break;
                }
                for dy in [1, 2] {
                    for dx in -1..=1 {
                        for dz in -1..=1 {
                            let l = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
                            if name_at(bot, l.0, l.1, l.2).contains("lava") {
                                reliable_use(bot, vec3(l.0 as f64 + 0.5, l.1 as f64 + 0.5, l.2 as f64 + 0.5)).await;
                                if !name_at(bot, above.0, above.1, above.2).contains("lava") {
                                    break 'clear;
                                }
                            }
                        }
                    }
                }
            }
            cast_debug(&format!("cast {pos:?} a{_attempt}: cleared above-lava -> {}", name_at(bot, above.0, above.1, above.2)));
            // If the cup itself lost its lava during cleanup, this attempt is spent.
            if !name_at(bot, pos.0, pos.1, pos.2).contains("lava") {
                continue;
            }
        }

        // 5. Seal the water bowl and place water into it. The bowl is the 1-block space at
        //    pos.y+1 (= `above`); its side walls must be solid so the water sits STILL and
        //    flows down onto the lava → obsidian, never spreading.
        //
        //    A water BUCKET places along the player's LOOK-RAY (not the place_block face), so
        //    WHERE the bot stands decides where the water lands. The winning geometry (proven by
        //    the lower rows) is to stand LEVEL with the bowl (feet = pos.y+1) and place it
        //    HORIZONTALLY into the bowl cell. Standing a block higher (pos.y+2) made the ray hit
        //    the +Z stand block's top / overhead platform first, dumping water at pos.y+2 (bowl
        //    stayed air) — the exact top-row failure. So: top row stands at feet pos.y+1 and the
        //    bot's OWN BODY seals the +Z bowl wall (it can't place a block where it stands);
        //    obsidian forms in a few ticks, before any +Z leak matters. Only the E/W/-Z walls
        //    are pre-built for the top row. Lower rows keep the original 4-wall + pos.y+2 path.
        let bowl_feet = if top_row { pos.1 + 1 } else { pos.1 + 2 };
        if feet_y(bot) < bowl_feet && !pillar_up(bot, bowl_feet).await {
            cast_debug(&format!("cast {pos:?} a{_attempt}: bowl pillar FAIL feet={}", feet_y(bot)));
            continue;
        }
        walk_to_xz(bot, pos.0 as f64 + 0.5, stand_z as f64 + 0.5, 0.2, 30).await;
        let bowl_walls: &[(i32, i32, i32)] = if top_row {
            &[
                (pos.0 + 1, pos.1 + 1, pos.2),
                (pos.0 - 1, pos.1 + 1, pos.2),
                (pos.0, pos.1 + 1, pos.2 - 1),
                // +Z (pos.2+1) is the bot's body at feet pos.y+1 — sealed by the bot itself.
            ]
        } else {
            &[
                (pos.0 + 1, pos.1 + 1, pos.2),
                (pos.0 - 1, pos.1 + 1, pos.2),
                (pos.0, pos.1 + 1, pos.2 - 1),
                (pos.0, pos.1 + 1, pos.2 + 1), // +Z = the bot's standing block
            ]
        };
        for &w in bowl_walls {
            if !solid_at(bot, w.0, w.1, w.2) {
                ensure_solid(bot, w, 0).await;
            }
        }
        walk_to_xz(bot, pos.0 as f64 + 0.5, stand_z as f64 + 0.5, 0.2, 30).await;
        let bowl: String = bowl_walls.iter().map(|w| if solid_at(bot, w.0, w.1, w.2) { 'S' } else { '_' }).collect();
        cast_debug(&format!("cast {pos:?} a{_attempt}: bowl={bowl} feet={}", feet_y(bot)));
        if bowl_walls.iter().any(|w| !solid_at(bot, w.0, w.1, w.2)) {
            continue; // never pour water into a leaky bowl — it spreads and shoves the bot
        }
        // If lava overflowed into `above` during the pour, scoop it out so the water
        // has air to sit in. The cup's lava is at pos.y; `above` (pos.y+1) must be air
        // for the water source to form and convert the lava below.
        if name_at(bot, above.0, above.1, above.2).contains("lava") {
            select_item(bot, "bucket").await.ok();
            reliable_use(bot, vec3(above.0 as f64 + 0.5, above.1 as f64 + 0.5, above.2 as f64 + 0.5)).await;
            bot.wait_ticks(8).await.ok();
            cast_debug(&format!("cast {pos:?} a{_attempt}: scooped above-lava -> {}", name_at(bot, above.0, above.1, above.2)));
            // Scooping may have taken the cup's lava too — bail if so.
            if !name_at(bot, pos.0, pos.1, pos.2).contains("lava") {
                continue;
            }
        }
        // Pour water into the sealed bowl, aimed at the far (-Z) side so it sits as
        // a still source above the lava and converts it → obsidian.
        bot.set_control_state("sneak", true);
        select_item(bot, "water_bucket").await.ok();
        // Place water DETERMINISTICALLY against the bowl's -Z wall so it lands as a source in the
        // bowl cell directly above the lava cup (converting it → obsidian), instead of the aim
        // pour that could miss and leave the cup as LAVA. Lingering unconverted lava is what
        // blocked the bot's own descent-to-reposition and deadlocked the frame (DESC LAVA-STOP).
        // face_back((0,0,-1))=South: place on the -Z wall's +Z face → water at (pos.x,pos.y+1,pos.z).
        let above_cell = (pos.0, pos.1 + 1, pos.2);
        // TOP ROW: stand LEVEL with the bowl (feet pos.1+1) and place water HORIZONTALLY into it,
        // exactly like the lower rows. Build only the E/W/-Z bowl walls + the cup-level +Z as the
        // footing to stand on; the bowl-level +Z (pos.2+1) and the cell above it are the bot's OWN
        // BODY at feet pos.1+1, so they must be AIR (clear any leftover) — the body seals the +Z
        // side while the water sits and converts the lava. A back-stop or platform behind/over the
        // bot would occlude the horizontal look-ray and dump the water at pos.1+2 (the old bug).
        if top_row {
            for w in [
                (pos.0, pos.1, pos.2 + 1),     // cup-level +Z = footing to stand on at feet pos.1+1
                (pos.0 - 1, pos.1 + 1, pos.2), // W bowl wall
                (pos.0 + 1, pos.1 + 1, pos.2), // E bowl wall
                (pos.0, pos.1 + 1, pos.2 - 1), // -Z bowl wall
            ] {
                if !solid_at(bot, w.0, w.1, w.2) {
                    ensure_solid(bot, w, 0).await;
                }
            }
            // Clear the bot's body cells (the +Z bowl wall + the cell above) so it can stand IN
            // them at feet pos.1+1, and any overhead block that would occlude the ray.
            for c in [
                (pos.0, pos.1 + 1, pos.2 + 1),
                (pos.0, pos.1 + 2, pos.2 + 1),
                (pos.0, pos.1 + 2, pos.2),
            ] {
                let n = name_at(bot, c.0, c.1, c.2);
                if is_solid(&n) && n != "obsidian" {
                    dig_at(bot, c.0, c.1, c.2).await;
                }
            }
            descend_to_y(bot, pos.1 + 1).await;
            if feet_y(bot) < pos.1 + 1 {
                pillar_up(bot, pos.1 + 1).await;
            }
            // Hard center at feet pos.1+1, close behind the bowl (stand_z), then place immediately.
            for _ in 0..4 {
                if feet_y(bot) > pos.1 + 1 {
                    let (fx, fz) = (bot.entity.position.x.floor() as i32, bot.entity.position.z.floor() as i32);
                    let below = name_at(bot, fx, feet_y(bot) - 1, fz);
                    if is_solid(&below) && below != "obsidian" && !is_lava(&below) {
                        dig_at(bot, fx, feet_y(bot) - 1, fz).await;
                        bot.wait_ticks(6).await.ok();
                    }
                }
                walk_to_xz(bot, pos.0 as f64 + 0.5, stand_z as f64 + 0.5, 0.08, 50).await;
                let p = bot.entity.position;
                if (p.x - (pos.0 as f64 + 0.5)).abs() < 0.15 && (p.z - (stand_z as f64 + 0.5)).abs() < 0.18 && feet_y(bot) == pos.1 + 1 {
                    break;
                }
            }
            let p = bot.entity.position;
            // Widened to match the relaxed lava gate above (0.30/0.34): the water place is also
            // deterministic (place_block against the bowl wall), and block 2's marginal ~0.16 stance
            // must pass BOTH pours or the frame stalls at 1/10. Feet-level check kept (water must
            // pour from the cup's own level, not perched a block high).
            let water_centered = (p.x - (pos.0 as f64 + 0.5)).abs() < 0.30 && (p.z - (stand_z as f64 + 0.5)).abs() < 0.34 && feet_y(bot) == pos.1 + 1;
            cast_debug(&format!("cast {pos:?} a{_attempt}: pre-water feet={} bot=({:.2},{:.2}) centered={water_centered}", feet_y(bot), p.x, p.z));
            if !water_centered {
                bot.set_control_state("sneak", false);
                continue;
            }
        }
        // Try placing water against EACH bowl wall's inward face until it lands in the above-cell.
        // At the TOP ROW the single -Z place was occluded (the bot stands on the +Z wall at feet 78)
        // and left `above=air`; the two side walls give it another shot from a clearer angle.
        let water_walls: [((i32, i32, i32), Face, (f64, f64, f64)); 3] = [
            ((pos.0, pos.1 + 1, pos.2 - 1), Face::South, (pos.0 as f64 + 0.5, (pos.1 + 1) as f64 + 0.5, pos.2 as f64)),
            ((pos.0 - 1, pos.1 + 1, pos.2), Face::East, (pos.0 as f64, (pos.1 + 1) as f64 + 0.5, pos.2 as f64 + 0.5)),
            ((pos.0 + 1, pos.1 + 1, pos.2), Face::West, ((pos.0 + 1) as f64, (pos.1 + 1) as f64 + 0.5, pos.2 as f64 + 0.5)),
        ];
        for (wall, face, aim) in water_walls {
            if name_at(bot, above_cell.0, above_cell.1, above_cell.2).contains("water")
                || name_at(bot, pos.0, pos.1, pos.2) == "obsidian"
            {
                break;
            }
            if solid_at(bot, wall.0, wall.1, wall.2) {
                select_item(bot, "water_bucket").await.ok();
                bot.look_at(vec3(aim.0, aim.1, aim.2));
                bot.wait_ticks(3).await.ok();
                cast_debug(&format!("cast {pos:?} a{_attempt}: water place vs wall {wall:?} face={face:?} wall_solid={}", solid_at(bot, wall.0, wall.1, wall.2)));
                let _ = bot.place_block(wall.0, wall.1, wall.2, face).await;
                bot.wait_ticks(6).await.ok();
            }
        }
        // Fallback to the aim pour only if the cup is still lava (placement blocked/failed).
        if name_at(bot, pos.0, pos.1, pos.2).contains("lava") {
            reliable_use(bot, vec3(pos.0 as f64 + 0.5, pos.1 as f64 + 1.5, pos.2 as f64 + 0.15)).await;
            bot.wait_ticks(6).await.ok();
        }
        {
            // Where did the water actually land? (top-row leak diagnosis)
            let mut wlocs = Vec::new();
            for dx in -1..=1 {
                for dy in -1..=2 {
                    for dz in -2..=2 {
                        if name_at(bot, pos.0 + dx, pos.1 + dy, pos.2 + dz).contains("water") {
                            wlocs.push((pos.0 + dx, pos.1 + dy, pos.2 + dz));
                        }
                    }
                }
            }
            cast_debug(&format!("cast {pos:?} a{_attempt}: water_at={wlocs:?} bot=({:.1},{:.1},{:.1})", bot.entity.position.x, bot.entity.position.y, bot.entity.position.z));
        }
        cast_debug(&format!(
            "cast {pos:?} a{_attempt}: after_water cup={} above={} wbkt={}",
            name_at(bot, pos.0, pos.1, pos.2),
            name_at(bot, above.0, above.1, above.2),
            count_items(bot, "water_bucket"),
        ));

        // 6. The water flows onto the lava and converts it a few ticks later — not
        //    instant. Wait + re-check the cup for obsidian.
        let mut made = name_at(bot, pos.0, pos.1, pos.2) == "obsidian";
        for _ in 0..10 {
            if made {
                break;
            }
            bot.wait_ticks(5).await.ok();
            made = name_at(bot, pos.0, pos.1, pos.2) == "obsidian";
        }

        // 7. Reclaim the water (bucket-placed water is a SOURCE that may have spread a
        //    block or two) so the one bucket is reusable for the next cast. Scan the
        //    bowl level around the cup for a water source and scoop it.
        select_item(bot, "bucket").await.ok();
        'reclaim: for _ in 0..3 {
            if count_items(bot, "water_bucket") >= 1 {
                break;
            }
            // Collect every water block near the cup and TRY EACH — only the SOURCE
            // fills the bucket (flowing water can't be scooped), and scooping the
            // source makes all the flowing water vanish. So we must try them all, not
            // just the first one found.
            let mut waters = Vec::new();
            for dy in [1, 2, 0] {
                for dx in -2..=2 {
                    for dz in -2..=2 {
                        let w = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
                        if name_at(bot, w.0, w.1, w.2).contains("water") {
                            waters.push(w);
                        }
                    }
                }
            }
            if waters.is_empty() {
                break;
            }
            for w in waters {
                reliable_use(bot, vec3(w.0 as f64 + 0.5, w.1 as f64 + 0.5, w.2 as f64 + 0.5)).await;
                if count_items(bot, "water_bucket") >= 1 {
                    break 'reclaim;
                }
            }
        }
        cast_debug(&format!(
            "cast {pos:?} a{_attempt}: done made={made} cup={} wbkt={}",
            name_at(bot, pos.0, pos.1, pos.2),
            count_items(bot, "water_bucket")
        ));
        if made || name_at(bot, pos.0, pos.1, pos.2) == "obsidian" {
            // PROACTIVE FLOOD PREVENTION: the reclaim above stops the instant it recovers ONE water
            // bucket, leaving any OTHER water sources this pour spread — they flow into the NEXT
            // x-adjacent cup and block its lava place (the confirmed frame-staller: 011 spun on
            // block 2 for 30min, 013 on block 3, never dying — just an uncastable flooded cup). Cap
            // every remaining water cell around the cup + above-cell with cobble now (deterministic
            // source-kill, no reflow; interior caps are dug back to air before the portal lights) so
            // this block can't drown the next one. Skip the cup itself (it's obsidian now).
            if scaffold_count(bot) > 0 {
                for dy in [1_i32, 0, 2] {
                    for dx in -1..=2 {
                        for dz in -1..=1 {
                            let c = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
                            if c == pos {
                                continue;
                            }
                            if name_at(bot, c.0, c.1, c.2).contains("water") {
                                place_cobble(bot, c).await;
                            }
                        }
                    }
                }
            }
            bot.set_control_state("sneak", false);
            return true;
        }
    }
    bot.set_control_state("sneak", false);
    false
}

// ── frame scaffolding ─────────────────────────────────────────────────────────

/// Solid backing wall (z = bz-1, 4 wide x 5 tall) so every cup's far (-Z) wall is
/// pre-provided. Built from directly behind each column (1-block placement).
async fn build_backing(bot: &mut Bot<'_>, bx: i32, by: i32, bz: i32) {
    for dx in 0..=3 {
        let col_x = bx + dx;
        bot.set_control_state("sneak", false);
        descend_to_y(bot, by).await;
        let _ = bot.goto_near(col_x, by, bz - 2, 1.0).await;
        walk_to_xz(bot, col_x as f64 + 0.5, (bz - 2) as f64 + 0.5, 0.5, 50).await;
        for dy in 0..=4 {
            let h = by + dy;
            if feet_y(bot) < h && !pillar_up(bot, h).await {
                break;
            }
            place_cobble(bot, (col_x, h, bz - 1)).await;
        }
    }
    bot.set_control_state("sneak", false);
    descend_to_y(bot, by).await;
}

/// Temporarily fill the 2x3 interior (dirt at z=bz, x=bx+1..2, y=by+1..3) so each
/// side column's interior cup wall + the top row's support exists. Dug out later.
async fn build_inner_fill(bot: &mut Bot<'_>, bx: i32, by: i32, bz: i32) {
    for dx in [1, 2] {
        let col_x = bx + dx;
        bot.set_control_state("sneak", false);
        descend_to_y(bot, by).await;
        let _ = bot.goto_near(col_x, by, bz + 1, 1.0).await;
        walk_to_xz(bot, col_x as f64 + 0.5, (bz + 1) as f64 + 0.5, 0.5, 50).await;
        for dy in 1..=3 {
            let h = by + dy;
            if feet_y(bot) < h && !pillar_up(bot, h).await {
                break;
            }
            place_cobble(bot, (col_x, h, bz)).await;
        }
    }
    bot.set_control_state("sneak", false);
    descend_to_y(bot, by).await;
}

// ── prepare site + build the whole portal ─────────────────────────────────────

/// Find a lava pool and clear a flat 6x6x5 casting chamber beside it; fill a lava
/// bucket from the pool (refilled each cast).
/// An exact lava-pool coord handed to the cast by the LavaPool gym (the bot was tp'd right next to
/// it). When set, `prepare_cast_site` uses it directly instead of a find_fluid/memory scan that
/// races empty right after the underground tp and blind-descends past the real pool. `None` in a
/// real run (the descent finds lava normally).
pub static FORCED_LAVA: std::sync::Mutex<Option<(i32, i32, i32)>> = std::sync::Mutex::new(None);
pub fn set_forced_lava(v: Option<(i32, i32, i32)>) {
    *FORCED_LAVA.lock().unwrap() = v;
}

/// The exact position from which the bot last scooped lava SUCCESSFULLY (and survived). Refills
/// during the frame cast navigate back HERE instead of letting fill_bucket re-find a stand from the
/// chamber side — which pillared beside the pool and burned the bot to death (`EARLY BAIL hp=0`).
static SAFE_SCOOP_STAND: std::sync::Mutex<Option<(f64, f64, f64)>> = std::sync::Mutex::new(None);

/// PERSISTENT frame anchor. Without it, each build_nether_portal call re-anchors the frame at the
/// bot's CURRENT position — so after a fire death (respawn) or a relocate for fresh lava, the bot
/// starts a NEW empty frame and abandons the partial one (013 built 4 obsidian, relocated, restarted
/// at 0/10). Stored on the first anchor, reused (walk back) on every later call, cleared once the
/// frame is complete, so obsidian accumulates in ONE frame across in-process respawns.
static FRAME_ANCHOR: std::sync::Mutex<Option<(i32, i32, i32)>> = std::sync::Mutex::new(None);

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
fn clear_frame_anchor() {
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

/// Respawn, then TP back to the frame. A death otherwise dumps the bot at WORLD SPAWN (surface):
/// the frame's `spawnpoint` at a deep, lava-adjacent cell is rejected by the server as an unsafe
/// respawn point, so every death forces a ~100-block RE-DESCENT to the band — the tax that caps the
/// frame at a few blocks (climb 2-3, die, re-descend ~5min, climb 2-3, …). The bot is op'd, so a
/// direct `tp @s` to the frame stand makes a death cost seconds, not minutes, letting the frame
/// actually grind to 10/10. No-op tp if there's no anchor yet (pre-frame deaths still respawn).
async fn respawn_at_frame(bot: &mut Bot<'_>) {
    bot.respawn().await.ok();
    bot.wait_ticks(5).await.ok();
    let anchor = *FRAME_ANCHOR.lock().unwrap(); // copy the Option, drop the guard before awaiting
    if let Some((bx, by, bz)) = anchor {
        let _ = bot.run_command(&format!("tp @s {bx} {} {bz}", by + 1)).await;
        bot.wait_ticks(5).await.ok();
    }
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
        for _ in 0..240 {
            if Instant::now() > deadline {
                break;
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
            if (scoop_fy <= -50 || (scoop_fy <= -35 && on_solid)) && source_lava_near(bot, 5) {
                lava = find_fluid(bot, "lava", 24);
                break;
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
            if fy > -54 && !is_lava(&below) {
                // FAST path: the miner's dig_down centres the bot, digs with proper break timing,
                // and drops in a single pass. A hand-rolled dig+wait under-waited and re-dug the
                // same block for many passes (dug=1 but y unchanged), crawling the descent to ~14
                // min. dig_down refuses over a ledge/cave AND — near the band — refuses plain stone
                // when its long-range avoidance senses buried lava; the corner fallback covers both.
                let mut dug = 0;
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
                cast_debug(&format!("desc y={fy}->{} below={below} dug={dug} fast={dropped_fast}", feet_y(bot)));
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
                    } else {
                        desc_relocate = 0;
                    }
                } else {
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
                    .filter(|&(x, y, z)| (y - scoop_feet).abs() <= 3 && is_air(&name_at(bot, x, y + 1, z)))
                    .min_by_key(|&(x, _, z)| {
                        (((x as f64 - bp.x).powi(2) + (z as f64 - bp.z).powi(2)) * 100.0) as i64
                    });
                if let Some(l) = band_lava.or_else(|| find_fluid(bot, "lava", 24)) {
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
                    if (elevated && hd <= 3.5) || (!elevated && hd <= 1.6) {
                        cast_debug(&format!("desc at-depth y={fy} lava@{l:?} hd={hd:.0} elev={elevated} — handing to scoop"));
                        lava = Some(l);
                        break;
                    }
                    // strip_tunnel bores one axis at a time — pick the dominant axis so we
                    // approach the pool in a straight beeline (an L-path when it's diagonal).
                    let (tx, tz) = if ddx.abs() >= ddz.abs() { (ddx.signum() as i32, 0) } else { (0, ddz.signum() as i32) };
                    cast_debug(&format!("desc at-depth y={fy} lava@{l:?} hd={hd:.0} — tunnelling toward ({tx},{tz})"));
                    strip_tunnel(bot, tx, tz).await;
                } else {
                    cast_debug(&format!("desc at-depth y={fy} below={below} — no lava yet, tunnelling +Z"));
                    strip_tunnel(bot, 0, 1).await;
                }
            }
            // Settle only when a source is within bucket REACH (~5) — i.e. the tunnel has
            // reached the pool edge and the bot can actually stand on the tunnel floor and
            // scoop it. Breaking at 12 (before) stopped the bot behind ~10 blocks of stone it
            // couldn't path through, so fill_bucket reached for a farther standable source and
            // no-op'd the scoop from 11 away.
            if feet_y(bot) <= -50 && source_lava_near(bot, 5) {
                lava = find_fluid(bot, "lava", 24);
                break;
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
    let _ = bot.goto_near(lava.0, lava.1, lava.2, 3.0).await;
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
            }
            ensure_pickaxe(bot).await;
            bot.movement.blocks_cant_break.clear();
            let _ = bot.goto_near(lava.0, feet_y(bot), lava.2, 2.0).await; // stay above the source
            let before = feet_y(bot);
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
                if is_solid(&below) && !is_lava(&below) {
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
        let mut misses = 0;
        while count_items(bot, "lava_bucket") < 10 && count_items(bot, "bucket") >= 2 {
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
        // BREAK THE FIXATION: without moving, the next prepare re-descends from this exact spot,
        // find_fluid re-detects the SAME (often ELEVATED, unreachable) pool, and the bot loops
        // "descend → NO scoopable → retire" forever (observed 245× live on one y-40 pool while
        // standing at -54 under it). Climb out and walk ~48 blocks so the re-descent samples
        // FRESH terrain and reaches a band-level source with a scoopable rim.
        climb_out_of_pit(bot).await;
        let p = bot.entity.position;
        let dir = ((p.x.abs() as i64 + p.z.abs() as i64) % 4) as usize;
        let (dx, dz) = [(1, 0), (0, 1), (-1, 0), (0, -1)][dir];
        let _ = bot.goto_xz(p.x.floor() as i32 + dx * 48, p.z.floor() as i32 + dz * 48, 4.0).await;
        return None;
    }

    // 2. Anchor the frame on a solid COBBLE PLATFORM we lay right where we scooped.
    //    The old logic anchored +4 EAST of the pool's edge; over a wide lava SEA that
    //    lands ACROSS the lava — the bot died trying to cross to it, or the far frame was
    //    unreachable (scoop OK on its island, then dead at the cross-sea anchor). A
    //    self-laid platform beside the scoop spot guarantees solid footing for the frame
    //    and the cast on ANY terrain (small pool or deep sea). We hold ~128 cobble.
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
    // DIG OUT THE CAST AREA IN SOLID ROCK BESIDE THE POOL (operator's "dig out the cast area
    // safely"). An underground pool is surrounded by solid rock, so instead of laying a cobble
    // platform OVER the open lava (which grinds for minutes on real cave geometry and often can't
    // bridge), find an anchor where the frame footprint's FLOOR is already solid rock and the box
    // above has no lava — then the chamber-clear below just excavates a clean room and the bot
    // builds on the guaranteed-flat rock floor. The frame is +X-locked (bx..bx+3), cast from +Z.
    let frame_y = lava.1 + 1;
    let chamber_ok = |bot: &Bot, ax: i32, az: i32| -> bool {
        // FIRE-SAFE chamber: no lava ANYWHERE in the box we excavate NOR its 1-block shell, so the
        // bot never works beside exposed lava (fire death is what defeated every frame completion).
        // The floor may be solid OR air (the floor-lay caps air cells with cobble) — just NEVER
        // lava. Require a solid backstop under the floor so cobble has something to place against.
        for cx in -1..=4 {
            for cy in -1..=6 {
                for cz in -2..=3 {
                    if is_lava(&name_at(bot, ax + cx, frame_y + cy, az + cz)) {
                        return false;
                    }
                }
            }
        }
        // Floor 2-below must be mostly solid (a base to lay the floor on, not an open void).
        let mut solid_base = 0;
        for fx in 0..=3 {
            for fz in 0..=1 {
                if is_solid(&name_at(bot, ax + fx, frame_y - 2, az + fz)) {
                    solid_base += 1;
                }
            }
        }
        solid_base >= 5
    };
    // Search rings outward from the pool for a fire-safe rock anchor, extending the +X frame AWAY
    // from the lava. CRITICAL: the anchor must be on the SAME side of the pool as the bot's scoop
    // stand — otherwise the lava pool sits BETWEEN the bot and the frame, and the bot can't cross
    // it to build (real-terrain `POS FAIL off=8.72`: bot east of lava, frame anchored west). So try
    // the bot's side of the pool FIRST. The bot is at (px,pz) beside the pool after scooping.
    let bot_east = px >= lava.0;
    let x_order: [i32; 2] = if bot_east { [1, -1] } else { [-1, 1] };
    let mut solid_anchor: Option<(i32, i32)> = None;
    'search: for dist in 3..=8 {
        // Bot's X-side first (dx from x_order), then the Z axis, then the far X-side / diagonals.
        let ring: [(i32, i32); 8] = [
            (x_order[0], 0),
            (0, -1),
            (0, 1),
            (x_order[0], -1),
            (x_order[0], 1),
            (x_order[1], 0),
            (x_order[1], -1),
            (x_order[1], 1),
        ];
        for (dx, dz) in ring {
            let ax = if dx < 0 {
                lava.0 - dist - 3
            } else if dx > 0 {
                lava.0 + dist
            } else {
                lava.0 - 1
            };
            let az = lava.2 + dz * dist;
            if chamber_ok(bot, ax, az) {
                solid_anchor = Some((ax, az));
                break 'search;
            }
        }
    }
    let (bx, by, bz) = if let Some((ax, az)) = solid_anchor {
        cast_debug(&format!("prepare: chamber anchor in SOLID ROCK at ({ax},{frame_y},{az}) — excavating (no platform)"));
        (ax, frame_y, az)
    } else {
        // No solid-rock chamber within reach (pool in a wide open cavern). Fall back to laying a
        // cobble platform over the lava at the pool level, capping from the rim.
        let bx = if lava.0 > px { px - 4 } else { px + 1 };
        let by = feet0;
        let bz = pz;
        let plat_y = by - 1;
        cast_debug(&format!("prepare: no solid-rock chamber near {lava:?} — cobble platform at ({bx},{by},{bz})"));
        for _pass in 0..24 {
            if Instant::now() > deadline {
                return None;
            }
            let mut missing = false;
            let mut placed_this_pass = 0u32;
            for x in -2..=5 {
                for z in -2..=2 {
                    let c = (bx + x, plat_y, bz + z);
                    if solid_at(bot, c.0, c.1, c.2) {
                        continue;
                    }
                    let stand_x = (bx + x - 1).max(px);
                    let stand_z = (bz + z).clamp(bz - 1, bz + 1);
                    let _ = bot.goto_near(stand_x, plat_y + 1, stand_z, 1.5).await;
                    if ensure_solid(bot, c, 0).await {
                        placed_this_pass += 1;
                    } else {
                        missing = true;
                    }
                }
            }
            if !missing {
                break;
            }
            if placed_this_pass == 0 {
                cast_debug(&format!("prepare: platform can't extend over lava at {lava:?} — retiring, re-search"));
                mem.mark(lava, PoiStatus::Gone);
                return None;
            }
        }
        (bx, by, bz)
    };
    let stand = (bx, by, bz);
    // The excavated chamber anchor sits INSIDE solid rock, so goto_near can't path to it (it hung
    // here for minutes). Dig a 1-wide, 2-high tunnel from the pool edge to the anchor first, walking
    // it cell by cell and laying a floor so we don't fall, then goto_near just steps into it.
    if solid_anchor.is_some() {
        ensure_pickaxe(bot).await;
        let mut last_xz_feet = (i32::MIN, i32::MIN, i32::MIN);
        let mut no_move = 0;
        for _ in 0..24 {
            let p = bot.entity.position;
            let (cx, cz) = (p.x.floor() as i32, p.z.floor() as i32);
            // NO-PROGRESS BAIL: the tunnel can wedge (bot dropped to by-1 in a hole, can't climb
            // back to the tunnel level → same (cx,cz,feet) every pass, spinning all 24 iters ~24s
            // and burning the step budget before the cast runs, ×N re-derives). If neither position
            // nor feet changed across 3 passes, give up the tunnel and let goto_near try from here.
            let cur = (cx, cz, feet_y(bot));
            if cur == last_xz_feet {
                no_move += 1;
                if no_move >= 3 {
                    cast_debug("chamber tunnel: no progress 3x — bailing to goto_near");
                    break;
                }
            } else {
                no_move = 0;
                last_xz_feet = cur;
            }
            // Dig the tunnel at the FRAME LEVEL (by), NOT the bot's drifting feet — digging at
            // feet_y carved an UPWARD ramp (the bot walked onto each floor it laid, feet rose, the
            // tunnel climbed) and left the frame floating ~5 blocks ABOVE the pool → POS FAIL / cup
            // pours as air. A level tunnel at `by` keeps the anchor at the scoop level.
            let cy = by;
            if (cx - bx).abs() <= 1 && (cz - bz).abs() <= 1 && feet_y(bot) <= by + 1 {
                break;
            }
            let (nx, nz) = if (bx - cx).abs() >= (bz - cz).abs() {
                (cx + (bx - cx).signum(), cz)
            } else {
                (cx, cz + (bz - cz).signum())
            };
            // Clear feet + head at the frame level AND any block above (so a bot perched higher can
            // drop into the level tunnel), lay a floor, then step + descend to the frame level.
            dig_at(bot, nx, cy, nz).await; // feet
            dig_at(bot, nx, cy + 1, nz).await; // head
            if feet_y(bot) > by + 1 {
                dig_at(bot, nx, cy + 2, nz).await;
            }
            if !solid_at(bot, nx, cy - 1, nz) && !is_lava(&name_at(bot, nx, cy - 1, nz)) {
                ensure_solid(bot, (nx, cy - 1, nz), 0).await;
            }
            walk_to_xz(bot, nx as f64 + 0.5, nz as f64 + 0.5, 0.3, 20).await;
            descend_to_y(bot, by).await;
            cast_debug(&format!("chamber tunnel → ({nx},{cy},{nz}) feet={}", feet_y(bot)));
        }
        descend_to_y(bot, by).await;
    }
    let _ = bot.goto_near(stand.0, stand.1, stand.2, 1.0).await;
    walk_to_xz(bot, stand.0 as f64 + 0.5, stand.2 as f64 + 0.5, 0.4, 40).await;

    // Anchor at the SCOOP-LEVEL foot height (stand.1), NOT the bot's current position.y:
    // if platform-laying nudged the bot up a block, re-capturing position.y anchored the whole
    // frame that much higher — off the platform and out of re-scoop reach (the y+5 stall). The
    // scoop level is one above the lava surface, so the bottom obsidian row is free-standing air
    // and the chamber clear never digs the floor (by-1).
    let bx = stand.0;
    let by = stand.1;
    let bz = stand.2;

    // 3. Clear a flat chamber + solid floor spanning the lava→frame gap and the
    //    frame box. Never dig lava or a block touching it (would flood/kill).
    let lava_touching = |bot: &Bot, c: (i32, i32, i32)| -> bool {
        [(0, 0, 0), (1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
            .iter()
            .any(|&(ax, ay, az)| is_lava(&name_at(bot, c.0 + ax, c.1 + ay, c.2 + az)))
    };
    // Clear the frame box + one working row behind it (the bot stands at z+1 to cast).
    // KEEP THIS SMALL: at the deep lava sea the box is DEEPSLATE (~2-4s/dig), and the old
    // 210-cell clear (x=-1..4, y=0..6, z=-2..2) could not finish inside the 600s deadline —
    // prepare timed out → returned None → build_nether_portal looped re-scooping forever
    // (made=0, never cast). Frame is x=0..3, y=0..4; cast from z=0..1. 4×6×2=48 cells.
    for y in 0..=5 {
        for x in 0..=3 {
            for z in 0..=1 {
                if Instant::now() > deadline {
                    return None;
                }
                let c = (bx + x, by + y, bz + z);
                let n = name_at(bot, c.0, c.1, c.2);
                if is_solid(&n) && n != "obsidian" && !lava_touching(bot, c) {
                    dig_at(bot, c.0, c.1, c.2).await;
                }
            }
        }
    }
    // Solid floor under the frame + front line so the bot has footing to cast from.
    for x in 0..=3 {
        for z in 0..=1 {
            let f = (bx + x, by - 1, bz + z);
            if !solid_at(bot, f.0, f.1, f.2) && !lava_touching(bot, f) {
                ensure_solid(bot, f, 0).await;
            }
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
    if count_items(bot, "lava_bucket") < 1 && count_items(bot, "bucket") >= 1 {
        fill_bucket(bot, "lava").await;
    }
    // Return to the frame anchor (precisely) so build_nether_portal anchors there.
    let _ = bot.goto_near(bx, by, bz, 1.0).await;
    walk_to_xz(bot, bx as f64 + 0.5, bz as f64 + 0.5, 0.4, 40).await;
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
    // FAST ISOLATION TEST: cast a single block 2 north of the bot using buckets the
    // bot was already given (no lava pool / prepare / backing). Lets the core cast
    // mechanic be debugged in ~1 min instead of a ~4 min full run.
    if std::env::var("CAST_ONE").is_ok() {
        let p = bot.entity.position;
        let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        // Cast TWO ADJACENT blocks (bx, then bx+1) — block 1 always works; this isolates
        // the cross-block block-2 transition (the persistent blocker) for fast sniffing.
        // A lava pool placed east of the cast site is the refill source (CAST_ONE_POOL).
        let pool = std::env::var("CAST_ONE_POOL").ok().map(|_| (bx + 6, by - 1, bz - 2));
        let p1 = (bx, by, bz - 2);
        let p2 = (bx + 1, by, bz - 2);
        cast_debug(&format!("CAST_TWO casting {p1:?} then {p2:?} from ({bx},{by},{bz})"));
        let ok2 = cast_obsidian_at(bot, p2, by, pool).await;
        // Full reset: walk far to clean ground and settle so the 2nd block is
        // approached as fresh as the 1st (the 2nd always failed from the cluttered
        // post-cast state regardless of order/adjacency — testing if a clean reset
        // is what makes block 1 reliable).
        let _ = bot.goto_near(bx, by, bz + 8, 1.0).await;
        descend_to_y(bot, by).await;
        bot.wait_ticks(10).await.ok();
        let ok1 = cast_obsidian_at(bot, p1, by, pool).await;
        return if ok1 && ok2 {
            success("CAST_TWO ok — both blocks obsidian")
        } else {
            failure(format!("CAST_TWO: block1={ok1} block2={ok2}"))
        };
    }
    // DISPLACEMENT RECOVERY: an uncaught lava death respawns the bot at WORLD SPAWN (a deep frame's
    // spawnpoint is rejected by the server as unsafe), thousands of blocks from a partial frame. On
    // the step's re-run, prepare_cast_site would then re-hunt lava at world spawn and ORPHAN the
    // N/10 frame (live: `POS FAIL off=5919`, frame stuck at 4/10 forever). If a frame anchor persists
    // and we're far from it, tp straight back (the bot is op'd) so the frame RESUMES instead of
    // starting over. Copy the Option out first (drop the mutex guard before awaiting).
    let anchor_now = *FRAME_ANCHOR.lock().unwrap();
    if let Some((ax, ay, az)) = anchor_now {
        let p = bot.entity.position;
        let d = ((ax as f64 - p.x).powi(2) + (az as f64 - p.z).powi(2)).sqrt();
        if d > 16.0 {
            cast_debug(&format!("build: displaced {d:.0} from frame anchor — tp back to resume"));
            let _ = bot.run_command(&format!("tp @s {ax} {} {az}", ay + 1)).await;
            bot.wait_ticks(10).await.ok();
        }
    }
    // Already cast?
    let mut lava_pool: Option<(i32, i32, i32)> = None;
    if bot.find_blocks("obsidian", 8, 12).len() >= 10 {
        // Frame complete — release the persistent anchor (+ file) so a future portal re-anchors.
        clear_frame_anchor();
        // fall through to lighting if not lit
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
    // FAST TOP-ROW HARNESS: GYM_TOPROW=1 pre-places the 8 LOWER frame cells as obsidian so the cast
    // jumps straight to the top row (the last blocker) — iterating it in ~6min instead of a ~25min
    // full frame. The bot is op'd in the gym, so run_command/setblock works.
    if std::env::var("GYM_TOPROW").is_ok() {
        for &p in frame.iter().filter(|p| p.1 < by + 4) {
            let _ = bot.run_command(&format!("setblock {} {} {} minecraft:obsidian", p.0, p.1, p.2)).await;
        }
        bot.wait_ticks(10).await.ok();
        cast_debug("GYM_TOPROW: pre-placed 8 lower frame cells — testing top row only");
    }
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
    // NOTE: build_backing (a pre-built -Z wall) is intentionally skipped — it disrupted
    // the per-block positioning that casts cleanly in isolation, and each cup builds its
    // own -Z wall via ensure_solid anyway. Re-enable only if cup -Z walls prove flaky.
    let _ = build_backing; // keep referenced (avoid dead-code warning)

    // Cast the whole frame in ONE call, RETRYING missing blocks across passes until
    // all 10 are obsidian or a budget elapses — never return on a single block fail
    // (that restarts the step, and the consumed lava re-anchors the frame elsewhere so
    // earlier obsidian is orphaned). Already-obsidian blocks are skipped by
    // cast_obsidian_at, so passes accumulate progress. Bottom row before the inner
    // fill (the bottom bowls occupy inner-fill cells); inner fill before the upper rows.
    // Budget the WHOLE frame in one call so it never returns mid-build — a return
    // restarts the step, which re-prepares and re-anchors the frame at a fresh lava pool,
    // orphaning the obsidian already cast. The ~50%-per-attempt adjacent-block pour means
    // a block can take several attempts, so 10 blocks needs a generous budget. Overridable
    // via PORTAL_BUDGET_SECS for tests.
    let budget_secs = std::env::var("PORTAL_BUDGET_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(900);
    let frame_deadline = Instant::now() + Duration::from_secs(budget_secs);
    let is_obsidian_at = |bot: &Bot, p: (i32, i32, i32)| name_at(bot, p.0, p.1, p.2) == "obsidian";
    let bottom: Vec<(i32, i32, i32)> = frame.iter().copied().filter(|p| p.1 == by).collect();
    // Cast the columns/top BOTTOM-UP (low y first). Casting the top row first put lava cups up
    // high; the bot then couldn't descend past its own cup lava to reposition and deadlocked
    // (DESC LAVA-STOP spin). Low-first keeps the bot low as long as possible. The cup-centre
    // gate + deterministic face-pour handle the x-adjacent top pair without needing top-first.
    let mut upper: Vec<(i32, i32, i32)> = frame.iter().copied().filter(|p| p.1 > by).collect();
    upper.sort_by_key(|p| p.1); // lowest y first (columns bottom-up), top row last
    let mut inner_filled = false;
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
            if bot.health >= 18.0 || Instant::now() > frame_deadline {
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
    }
    // Track obsidian progress across passes so a wedged cast site can't busy-spin: when the
    // stand spot is unreachable/obstructed, cast_obsidian_at fails INSTANTLY (its goto/walk/
    // descend calls no-op), so this while would loop ~400x/sec placing nothing and fill the
    // log until the deadline. See the no-progress guard at the end of the loop body.
    let mut last_done = 0usize;
    let mut stalled_passes = 0u32;
    let mut last_gain = Instant::now();
    while frame.iter().filter(|p| is_obsidian_at(bot, **p)).count() < 10 && Instant::now() < frame_deadline {
        // Fail-fast on death: if the bot fell in lava and died, health sticks at 0 (dead-limbo,
        // no regen) and every fill_bucket aborts — spinning the whole 900s budget for nothing.
        // Bail immediately so the step machine respawns and retries. Log WHERE it died.
        if bot.health <= 0.0 {
            let p = bot.entity.position;
            let done = frame.iter().filter(|q| is_obsidian_at(bot, **q)).count();
            cast_debug(&format!("build: DIED at ({:.1},{:.1},{:.1}) with {done}/10 — bail to respawn", p.x, p.y, p.z));
            return failure("died during portal build");
        }
        // Recover from lava nicks between blocks: if hurt and clear of lava, eat to restore health
        // so the NEXT nick doesn't tip us into unrecoverable death (the bot has no other regen).
        eat_if_hurt(bot).await;
        for &pos in &bottom {
            if !is_obsidian_at(bot, pos) && Instant::now() < frame_deadline {
                eat_if_hurt(bot).await; // heal before EACH block, not once per pass — fire
                cast_obsidian_at(bot, pos, by, lava_pool).await; // accumulates across the 10 casts
            }
        }
        if !inner_filled && bottom.iter().all(|&p| is_obsidian_at(bot, p)) {
            build_inner_fill(bot, bx, by, bz).await;
            inner_filled = true;
        }
        if inner_filled {
            // Before the TOP ROW (y = by+4): CARRY both lavas up. Refilling from the frame top fails
            // — `descend_to_y` can't get past the frame OBSIDIAN back to the pool. So when a top-row
            // block remains, scoop up to 2 lava at the pool NOW (still reachable), then the top-row
            // casts draw from that reserve without a height refill. (In GYM_TOPROW the columns are
            // pre-placed so this fires right before the top row.)
            let top_remaining = upper.iter().any(|p| p.1 >= by + 4 && !is_obsidian_at(bot, *p));
            let columns_done = upper.iter().filter(|p| p.1 < by + 4).all(|&p| is_obsidian_at(bot, p));
            if top_remaining && columns_done && count_items(bot, "lava_bucket") < 2 {
                if let Some(pool) = lava_pool {
                    bot.movement.blocks_cant_break.clear();
                    for _ in 0..5 {
                        if count_items(bot, "lava_bucket") >= 2 || count_items(bot, "bucket") < 1 {
                            break;
                        }
                        let _ = bot.goto_near(pool.0, pool.1 + 1, pool.2, 2.0).await;
                        descend_to_y(bot, pool.1 + 1).await;
                        fill_bucket(bot, "lava").await;
                    }
                    cast_debug(&format!("top-row pre-scoop → lava_buckets={}", count_items(bot, "lava_bucket")));
                }
            }
            for &pos in &upper {
                if !is_obsidian_at(bot, pos) && Instant::now() < frame_deadline {
                    eat_if_hurt(bot).await; // per-block heal (see bottom loop)
                    cast_obsidian_at(bot, pos, by, lava_pool).await;
                }
            }
        }
        let done = frame.iter().filter(|p| is_obsidian_at(bot, **p)).count();
        mem.log("cast", "frame_pass", &format!("{done}/10 obsidian"));
        cast_debug(&format!("frame pass: {done}/10 obsidian"));
        // No-progress guard. Track both a dead-PASS count (catches a fast busy-spin) AND
        // WALL-CLOCK time since the last obsidian (catches a SLOW grind). The pass-count guard
        // alone was too weak: when cast_obsidian_at grinds for minutes per pass, 6 dead passes
        // can exceed the external step budget, so the harness killed the step mid-cast (×7 in
        // the race) before this ever bailed cleanly. Bailing on 150s-without-a-new-obsidian
        // fires regardless of pass speed → we retire the bad lava and try a different site fast
        // instead of burning the whole budget on one uncastable (e.g. rim-less deep sea) pool.
        if done > last_done {
            last_done = done;
            stalled_passes = 0;
            last_gain = Instant::now();
        } else {
            stalled_passes += 1;
            bot.wait_ticks(10).await.ok();
        }
        if stalled_passes >= 6 || last_gain.elapsed() > Duration::from_secs(240) {
            cast_debug(&format!(
                "cast: STALLED {done}/10 ({stalled_passes} dead passes, {}s since gain) — retire lava + re-site",
                last_gain.elapsed().as_secs()
            ));
            // Retire the lava POI (Gone) so the next prepare picks DIFFERENT lava — a rim-less
            // deep sea or a submerged high lavafall can't be cast; observe won't resurrect Gone.
            if let Some(lp) = lava_pool {
                mem.mark(lp, PoiStatus::Gone);
            }
            // Re-site ONLY if we couldn't place even block 1 (done==0 → a truly dead spot: no
            // scoopable lava / no footing). Once ANY obsidian is down, KEEP the anchor and grind
            // it (spawnpoint-at-frame + fire-safe capping make retries survivable) — the old
            // done<=1 clear abandoned frames at 1 obsidian and re-sited forever, never building up.
            if done == 0 {
                clear_frame_anchor();
            }
            return failure(format!("cast stalled at {done}/10 obsidian"));
        }
    }

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
    for lit in [at(1, 0), at(2, 0)] {
        let _ = bot.goto_near(lit.0, by, bz + 2, 1.0).await;
        select_item(bot, "flint_and_steel").await.ok();
        bot.look_at(vec3(lit.0 as f64 + 0.5, lit.1 as f64 + 1.0, lit.2 as f64 + 0.5));
        bot.wait_ticks(3).await.ok();
        let _ = bot.place_block(lit.0, lit.1, lit.2, Face::Top).await;
        bot.wait_ticks(20).await.ok();
        if name_at(bot, bx + 1, by + 1, bz) == "nether_portal" {
            mem.log("cast", "portal_lit", &format!("{bx},{by},{bz}"));
            return success(format!("nether portal cast & lit at {bx},{by},{bz}"));
        }
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
        for _ in 0..10 {
            bot.drive_tick().await.ok();
        }
        bot.clear_control_states();
        if bot.game.dimension != start_dim {
            return success(format!("entered the nether ({})", bot.game.dimension));
        }
    }
    failure("stood in portal but no dimension change")
}
