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

fn feet_y(bot: &Bot) -> i32 {
    bot.entity.position.y.floor() as i32
}

/// The cheap throwaway block we scaffold/mould with (cobble preferred, then dirt).
fn build_block(bot: &Bot) -> &'static str {
    if count_items(bot, "cobblestone") > 0 {
        "cobblestone"
    } else {
        "dirt"
    }
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
        let (nx, nz) = dirs8
            .iter()
            .map(|(dx, dz)| (s.0 + dx, s.2 + dz))
            .find(|&(nx, nz)| {
                !is_lava(&name_at(bot, nx, s.1, nz)) && !is_lava(&name_at(bot, nx, s.1 - 1, nz))
            })
            .unwrap_or((s.0, s.2));
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
        let _ = bot.goto_near(stand.0 as i32, stand.1 as i32, stand.2 as i32, 1.0).await;
        walk_to_xz(bot, stand.0, stand.2, 0.4, 50).await;
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
        for t in &targets {
            for dy in [0.6_f64, 0.2, 0.9] {
                reliable_use(bot, vec3(t.0 as f64 + 0.5, t.1 as f64 + dy, t.2 as f64 + 0.5)).await;
                if count_items(bot, &filled_bucket_name) > 0 {
                    cast_debug(&format!("fill {fluid}: OK (round {round})"));
                    return true;
                }
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
        cast_debug(&format!("cast {pos:?} ENTER bot=({:.1},{:.1},{:.1})", p.x, p.y, p.z));
    }

    for _attempt in 0..5 {
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
        // 1. Top up both buckets first (fill walks to the pool).
        // Refill lava from the KNOWN pool — navigate back to it first so the local
        // scan in fill_bucket always sees it (scanning from wherever the previous
        // block left the bot is what kept failing).
        if count_items(bot, "lava_bucket") < 1 {
            if let Some(pool) = lava_pool {
                let _ = bot.goto_near(pool.0, pool.1 + 1, pool.2, 2.0).await;
            }
        }
        if count_items(bot, "lava_bucket") < 1 && !fill_bucket(bot, "lava").await {
            return false;
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
        descend_to_y(bot, base_y).await; // off the previous pour-pillar
        for try_pos in 0..3 {
            let _ = bot.goto_near(pos.0, base_y, stand_z, 1.0).await;
            descend_to_y(bot, base_y).await;
            walk_to_xz(bot, pos.0 as f64 + 0.5, stand_z as f64 + 0.5, 0.4, 60).await;
            if dist_from_stand(bot) <= 1.2 && feet_y(bot) <= base_y + 1 {
                break;
            }
            // Stage from a clear spot behind the working line, then re-approach.
            let _ = bot.goto_near(pos.0, base_y, stand_z + 3, 1.0).await;
            descend_to_y(bot, base_y).await;
            if try_pos == 2 {
                let p = bot.entity.position;
                cast_debug(&format!("cast {pos:?} a{_attempt}: POS FAIL off={:.2} feet={} bot=({:.1},{:.1},{:.1})", dist_from_stand(bot), feet_y(bot), p.x, p.y, p.z));
            }
        }
        if dist_from_stand(bot) > 1.4 {
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
        let centered = (p.x - (pos.0 as f64 + 0.5)).abs() < 0.15 && (p.z - pour_z).abs() < 0.18;
        cast_debug(&format!(
            "cast {pos:?} a{_attempt}: pre-lava feet={} bot=({:.2},{:.2}) centered={centered}",
            feet_y(bot), p.x, p.z
        ));
        if !centered {
            continue; // don't pour from off-centre — it'll miss, damage us, and flood
        }
        select_item(bot, "lava_bucket").await.ok();
        // Aim straight down into the open cup, but at the bot's OWN x (not the fixed cup
        // centre): the pour is so geometry-sensitive that a 0.04-block x drift sends the
        // lava onto the wall. Aiming at the bot's actual x makes the look yaw≈0 (dead
        // north) so the ray drops into the bot's own cell — which the hard-centering put
        // over the cup — independent of the small x offset. Vary pour depth by attempt.
        let bx_aim = bot.entity.position.x;
        let lava_aim = match _attempt {
            0 => vec3(bx_aim, pos.1 as f64 + 0.2, pos.2 as f64 + 0.5),
            1 => vec3(bx_aim, pos.1 as f64 + 0.0, pos.2 as f64 + 0.4),
            _ => vec3(bx_aim, pos.1 as f64 + 0.4, pos.2 as f64 + 0.6),
        };
        reliable_use(bot, lava_aim).await;
        bot.wait_ticks(8).await.ok();
        cast_debug(&format!("cast {pos:?} a{_attempt}: after_lava cup_block={}", name_at(bot, pos.0, pos.1, pos.2)));
        // If the lava missed the cup, this attempt is wasted AND the misplaced lava is
        // likely at the bot's own feet (the +Z wall top) — ESCAPE it (sprint-jump back
        // south) before it burns us, then SCOOP the misplaced source so it doesn't flood
        // the work area across retries (an uncleaned flood is what eventually traps the
        // bot and deadlocks its buckets). The escape recovers the empty bucket; scooping
        // the stray source refills it to lava, ready for the next attempt.
        if !name_at(bot, pos.0, pos.1, pos.2).contains("lava") {
            bot.set_control_state("sneak", false);
            bot.look_at(vec3(pos.0 as f64 + 0.5, pos.1 as f64, stand_z as f64 + 4.0));
            bot.set_control_state("forward", true);
            bot.set_control_state("sprint", true);
            bot.set_control_state("jump", true);
            for _ in 0..12 {
                bot.drive_tick().await.ok();
            }
            bot.clear_control_states();
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

        // 5. Seal the water bowl and pour water from pos.y+2 (port of steve's TS cast).
        //    The bowl is the 1-block space at pos.y+1 (= `above`); its 4 walls
        //    (E,W,N,+Z) must ALL be solid so the poured water sits as a STILL source
        //    and flows down onto the lava → obsidian, never spreading. The +Z wall is
        //    the block the bot stands on; pillar to pos.y+2 so that wall exists, then
        //    build/verify E,W,N, gate-check all 4, and pour from up high.
        if feet_y(bot) < pos.1 + 2 && !pillar_up(bot, pos.1 + 2).await {
            cast_debug(&format!("cast {pos:?} a{_attempt}: bowl pillar FAIL feet={}", feet_y(bot)));
            continue;
        }
        walk_to_xz(bot, pos.0 as f64 + 0.5, stand_z as f64 + 0.5, 0.2, 30).await;
        let bowl_walls = [
            (pos.0 + 1, pos.1 + 1, pos.2),
            (pos.0 - 1, pos.1 + 1, pos.2),
            (pos.0, pos.1 + 1, pos.2 - 1),
            (pos.0, pos.1 + 1, pos.2 + 1), // +Z = the bot's standing block
        ];
        for w in bowl_walls {
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
        reliable_use(bot, vec3(pos.0 as f64 + 0.5, pos.1 as f64 + 1.5, pos.2 as f64 + 0.15)).await;
        bot.wait_ticks(6).await.ok();
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
async fn prepare_cast_site(bot: &mut Bot<'_>, mem: &mut WorldMemory) -> Option<(i32, i32, i32)> {
    let deadline = Instant::now() + Duration::from_secs(600);

    // Let any pending block updates settle so the bot's local world is current
    // before we scan for lava (an RCON-placed / freshly-revealed pool may not be in
    // the world yet on the very first tick).
    bot.wait_ticks(10).await.ok();
    // 1. Locate visible lava; if none, dig down toward cave-lava depth and retry.
    // Keep the radius modest — a 30-block exposed scan is ~226k synchronous block
    // lookups that block the network loop past the keep-alive timeout (→ kick).
    let mut lava = find_fluid(bot, "lava", 16);
    {
        let p = bot.entity.position;
        cast_debug(&format!("prepare: at ({:.0},{:.0},{:.0}) lava={lava:?}", p.x, p.y, p.z));
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
            lava = find_fluid(bot, "lava", 16);
        }
    }
    if lava.is_none() {
        // No remembered lava in reach. Reach the DEEP lava zone (y≈-40..-54, where big
        // EXPOSED lava lakes live) using the ore-miner's PROVEN relocating descent — the
        // same machinery that reliably tunnels down through mixed stone/cave/aquifer
        // terrain to find iron. A hand-rolled straight dig-down stalls the instant it
        // meets an open cave or aquifer (it only digs solid), so it never gets deep. This
        // one relocates: dig_down straight, else stair-step in all 4 dirs, else strip-
        // tunnel to fresh ground. At depth, strip-tunnel to expose cavern walls. find_fluid
        // then locks onto an air-topped lava surface. (dig_down still avoids lava, so we
        // never dive in — we descend PAST the shallow pockets to the deep lakes.)
        bot.movement.blocks_cant_break.clear();
        const DIRS: [(i32, i32); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];
        let mut desc_fail = 0u32;
        for _ in 0..240 {
            if Instant::now() > deadline {
                break;
            }
            // Reached the lava zone? Stop and scoop the exposed pool — never dig or
            // relocate further into it (walking into it at y-1 is what killed the bot).
            if raw_lava_near(bot, 10) {
                lava = find_fluid(bot, "lava", 24);
                break;
            }
            if feet_y(bot) > -45 {
                // The descent digs via bot.dig directly, which uses whatever's in hand — and
                // the portal phase holds buckets/flint, so hard stone (andesite/granite) can't
                // be broken bare-handed and the descent stalls. Keep a pickaxe equipped.
                ensure_pickaxe(bot).await;
                let fy = feet_y(bot);
                let (px, pz) = (bot.entity.position.x.floor() as i32, bot.entity.position.z.floor() as i32);
                let below = name_at(bot, px, fy - 1, pz);
                let below2 = name_at(bot, px, fy - 2, pz);
                let mut descended = dig_down(bot).await;
                let mut how = if descended { "dig_down" } else { "-" };
                if !descended {
                    for &(dx, dz) in &DIRS {
                        if descend_step(bot, dx, dz).await {
                            descended = true;
                            how = "step";
                            break;
                        }
                    }
                }
                // Aquifer punch-through: when a water pocket (not lava) is what stalled the
                // descent, dig down THROUGH it rather than relocating forever.
                if !descended && !raw_lava_near(bot, 10) && punch_through_water(bot).await {
                    descended = true;
                    how = "punch";
                }
                // Cavern drop: a solid floor over an open cave blocks dig_down (fall-avoidance),
                // but caves are exactly where exposed lava lives — drop in when it's survivable.
                if !descended && !raw_lava_near(bot, 10) && drop_into_cavern(bot).await {
                    descended = true;
                    how = "cavern";
                }
                if descended {
                    desc_fail = 0;
                    cast_debug(&format!("desc y={fy}->{} via {how} (below={below})", feet_y(bot)));
                } else if raw_lava_near(bot, 10) {
                    // Descent blocked BY lava — grab it, don't relocate straight into it.
                    lava = find_fluid(bot, "lava", 24);
                    break;
                } else {
                    desc_fail += 1;
                    let (dx, dz) = DIRS[(desc_fail as usize / 2) % 4];
                    let dist = 6 + (desc_fail.min(8) as i32) * 4;
                    let p = bot.entity.position;
                    cast_debug(&format!("desc y={fy} STUCK below={below} below2={below2} → relocate#{desc_fail} ({dx},{dz})x{dist}"));
                    let _ = bot
                        .goto_xz(p.x.floor() as i32 + dx * dist, p.z.floor() as i32 + dz * dist, 2.0)
                        .await;
                }
            } else {
                strip_tunnel(bot, 0, 1).await; // at depth — expose fresh cavern walls
            }
            lava = find_fluid(bot, "lava", 20);
            if lava.is_some() {
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

    // Scoop the lava bucket NOW, while we're right next to the exposed pool. The anchor +
    // chamber-clear below moves the bot away and is slow (180-cell dig loop) — it was eating
    // the deadline before the scoop, so the bot reached the lava but never filled the bucket.
    // Getting the (critical, order-sensitive) lava_bucket here first is what unblocks the cast.
    if count_items(bot, "lava_bucket") < 1 && count_items(bot, "bucket") >= 1 {
        fill_bucket(bot, "lava").await;
        cast_debug(&format!("prepare: early scoop → lava_buckets={}", count_items(bot, "lava_bucket")));
    }

    // 2. Anchor the frame a fixed gap past the EAST edge of the WHOLE pool (scan +X
    //    from the found source until the lava ends), so the frame — which extends +X —
    //    never overlaps the pool. A fixed +6 lands inside a wide pool; the bot would
    //    fall onto the lava. The bot stands east on solid ground and refills by walking
    //    west to the pool.
    bot.set_control_state("sneak", false);
    let mut east_edge = lava.0;
    while east_edge < lava.0 + 24
        && (is_lava(&name_at(bot, east_edge + 1, lava.1, lava.2))
            || is_lava(&name_at(bot, east_edge + 1, lava.1 + 1, lava.2)))
    {
        east_edge += 1;
    }
    let stand = (east_edge + 4, lava.1 + 2, lava.2); // +2: surface, one above the lava
    cast_debug(&format!("prepare: pool east_edge={east_edge}, anchor x={}", stand.0));
    let _ = bot.goto_near(stand.0, stand.1, stand.2, 1.0).await;
    walk_to_xz(bot, stand.0 as f64 + 0.5, stand.2 as f64 + 0.5, 0.4, 40).await;

    // Anchor at the bot's FOOT level (one above the lava surface) so the bottom
    // obsidian row is free-standing air and the chamber clear never digs the FLOOR
    // (by-1) — digging it then re-laying cobble burned the whole cobble stock.
    let bx = stand.0;
    let by = bot.entity.position.y.floor() as i32;
    let bz = stand.2;

    // 3. Clear a flat chamber + solid floor spanning the lava→frame gap and the
    //    frame box. Never dig lava or a block touching it (would flood/kill).
    let lava_touching = |bot: &Bot, c: (i32, i32, i32)| -> bool {
        [(0, 0, 0), (1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]
            .iter()
            .any(|&(ax, ay, az)| is_lava(&name_at(bot, c.0 + ax, c.1 + ay, c.2 + az)))
    };
    // Clear just the frame box + the front working line (the bot stands at z+1 to
    // cast). Travel to/from the lava is left to the pathfinder, so we don't clear the
    // whole gap — a big clear is hundreds of slow per-cell ops that time the step out.
    for y in 0..=6 {
        for x in -1..=4 {
            for z in -2..=2 {
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
    for x in -1..=4 {
        for z in -1..=2 {
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
    // Already cast?
    let mut lava_pool: Option<(i32, i32, i32)> = None;
    if bot.find_blocks("obsidian", 8, 12).len() >= 10 {
        // fall through to lighting if not lit
    } else {
        if count_items(bot, "cobblestone") + count_items(bot, "dirt") < 30 {
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

    let bx = bot.entity.position.x.floor() as i32;
    let by = bot.entity.position.y.floor() as i32;
    let bz = bot.entity.position.z.floor() as i32;
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
    let upper: Vec<(i32, i32, i32)> = frame.iter().copied().filter(|p| p.1 > by).collect();
    let mut inner_filled = false;
    while frame.iter().filter(|p| is_obsidian_at(bot, **p)).count() < 10 && Instant::now() < frame_deadline {
        for &pos in &bottom {
            if !is_obsidian_at(bot, pos) && Instant::now() < frame_deadline {
                cast_obsidian_at(bot, pos, by, lava_pool).await;
            }
        }
        if !inner_filled && bottom.iter().all(|&p| is_obsidian_at(bot, p)) {
            build_inner_fill(bot, bx, by, bz).await;
            inner_filled = true;
        }
        if inner_filled {
            for &pos in &upper {
                if !is_obsidian_at(bot, pos) && Instant::now() < frame_deadline {
                    cast_obsidian_at(bot, pos, by, lava_pool).await;
                }
            }
        }
        let done = frame.iter().filter(|p| is_obsidian_at(bot, **p)).count();
        mem.log("cast", "frame_pass", &format!("{done}/10 obsidian"));
        cast_debug(&format!("frame pass: {done}/10 obsidian"));
    }

    // Open the 2x3 interior + the +Z approach (never dig obsidian).
    descend_to_y(bot, by).await;
    for dx in 1..=2 {
        for dy in 1..=3 {
            dig_at(bot, bx + dx, by + dy, bz).await;
            dig_at(bot, bx + dx, by + dy, bz + 1).await;
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
