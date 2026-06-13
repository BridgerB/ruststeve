//! Nether portal by obsidian casting (no diamond pickaxe) + portal entry.
//!
//! Port of steve's `tasks/portal/{cast,enter}.ts`. Each obsidian block is cast in
//! place: a fully-enclosed 1-block cup holds a lava source, then water poured into
//! the block directly above flows down and turns it to obsidian. The cup contains
//! the lava so it never reaches the bot. The 4x5 frame (10 obsidian, no corners) is
//! cast bottom-up, the 2x3 interior is dug out, and it's lit with flint & steel.

use std::time::{Duration, Instant};

use rustcraft::bot::{Bot, DriveStep, Face};
use rustcraft::vec3::{vec3, Vec3};

use crate::bot_utils::{count_items, select_item};
use crate::memory::WorldMemory;
use crate::types::{failure, success, StepResult};

// ── block classification ────────────────────────────────────────────────────

/// Debug log for the cast (to stderr → the bot log) when CRAFT_DEBUG is set.
fn cdbg(msg: &str) {
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

fn solid_at(bot: &Bot, x: i32, y: i32, z: i32) -> bool {
    is_solid(&name_at(bot, x, y, z))
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
        for (cx, cz) in cells {
            let n = name_at(bot, cx, f - 1, cz);
            if is_solid(&n) && n != "obsidian" {
                dig_at(bot, cx, f - 1, cz).await;
            }
        }
        bot.wait_ticks(8).await.ok();
    }
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
        walk_to_xz(bot, cell_x as f64 + 0.5, cell_z as f64 + 0.5, 0.2, 18).await;
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
    cdbg(&format!(
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
    let mut candidates = bot.find_exposed_blocks(fluid, 16, 64);
    if candidates.is_empty() {
        bot.wait_ticks(10).await.ok();
        candidates = bot.find_exposed_blocks(fluid, 16, 64);
    }
    let mut chosen: Option<((i32, i32, i32), (f64, f64, f64))> = None;
    'src: for src in candidates {
        if !is_air(&name_at(bot, src.0, src.1 + 1, src.2)) {
            continue; // need an open surface to scoop
        }
        // A spot to STAND ON within reach of the source, checking both the source's
        // own level (lava flush with the floor) AND one level up (a recessed pit —
        // the bot stands on the floor BESIDE the pit and scoops down). 8 directions.
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
            for sb_y in [src.1, src.1 + 1] {
                let (sx, sy, sz) = (src.0 + dx, sb_y, src.2 + dz);
                if solid_at(bot, sx, sy, sz)
                    && is_air(&name_at(bot, sx, sy + 1, sz))
                    && is_air(&name_at(bot, sx, sy + 2, sz))
                {
                    chosen = Some((src, (sx as f64 + 0.5, (sy + 1) as f64, sz as f64 + 0.5)));
                    break 'src;
                }
            }
        }
    }
    // Water doesn't burn, so as a last resort scoop it from directly above; never do
    // that for lava.
    let (src, stand) = match chosen {
        Some(c) => c,
        None => {
            if fluid == "lava" {
                cdbg(&format!(
                    "fill lava: NO edge source with a stand spot ({} {fluid} blocks seen)",
                    bot.find_exposed_blocks(fluid, 16, 64).len()
                ));
                return false;
            }
            let Some(s) = find_fluid(bot, fluid, 16) else {
                return false;
            };
            (s, (s.0 as f64 + 0.5, (s.1 + 1) as f64, s.2 as f64 + 0.5))
        }
    };
    let _ = bot.goto_near(stand.0 as i32, stand.1 as i32, stand.2 as i32, 1.0).await;
    walk_to_xz(bot, stand.0, stand.2, 0.4, 40).await;
    let p = bot.entity.position;
    let off = (p.x - stand.0).abs() + (p.z - stand.2).abs();
    cdbg(&format!(
        "fill {fluid}: src={src:?} stand=({:.1},{:.1}) bot=({:.1},{:.1},{:.1}) off={off:.2}",
        stand.0, stand.2, p.x, p.y, p.z
    ));
    if !select_item(bot, "bucket").await.unwrap_or(false) {
        cdbg(&format!("fill {fluid}: no empty bucket equipped"));
        return false;
    }
    for i in 0..6 {
        let dy = if i % 2 == 0 { 0.5 } else { 0.1 };
        reliable_use(bot, vec3(src.0 as f64 + 0.5, src.1 as f64 + dy, src.2 as f64 + 0.5)).await;
        if count_items(bot, &format!("{fluid}_bucket")) > 0 {
            cdbg(&format!("fill {fluid}: OK on try {i}"));
            return true;
        }
    }
    cdbg(&format!("fill {fluid}: 6 tries failed (off={off:.2})"));
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
        cdbg(&format!("cast {pos:?} ENTER bot=({:.1},{:.1},{:.1})", p.x, p.y, p.z));
    }

    for _attempt in 0..3 {
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
        let off_now = |bot: &Bot| {
            (bot.entity.position.x - (pos.0 as f64 + 0.5)).abs()
                + (bot.entity.position.z - (stand_z as f64 + 0.5)).abs()
        };
        descend_to_y(bot, base_y).await; // off the previous pour-pillar
        for try_pos in 0..3 {
            let _ = bot.goto_near(pos.0, base_y, stand_z, 1.0).await;
            descend_to_y(bot, base_y).await;
            walk_to_xz(bot, pos.0 as f64 + 0.5, stand_z as f64 + 0.5, 0.4, 60).await;
            if off_now(bot) <= 1.2 && feet_y(bot) <= base_y + 1 {
                break;
            }
            // Stage from a clear spot behind the working line, then re-approach.
            let _ = bot.goto_near(pos.0, base_y, stand_z + 3, 1.0).await;
            descend_to_y(bot, base_y).await;
            if try_pos == 2 {
                let p = bot.entity.position;
                cdbg(&format!("cast {pos:?} a{_attempt}: POS FAIL off={:.2} feet={} bot=({:.1},{:.1},{:.1})", off_now(bot), feet_y(bot), p.x, p.y, p.z));
            }
        }
        if off_now(bot) > 1.4 {
            bot.set_control_state("sneak", false);
            return false;
        }
        if feet_y(bot) < pos.1 && !pillar_up(bot, pos.1).await {
            cdbg(&format!("cast {pos:?} a{_attempt}: pillar1 FAIL feet={}", feet_y(bot)));
            continue;
        }
        cdbg(&format!(
            "cast {pos:?} a{_attempt}: positioned off={:.2} feet={} lava_b={} water_b={}",
            off_now(bot),
            feet_y(bot),
            count_items(bot, "lava_bucket"),
            count_items(bot, "water_bucket")
        ));

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
        cdbg(&format!("cast {pos:?} a{_attempt}: cup={cup} feet={}", feet_y(bot)));
        if walls.iter().any(|w| !solid_at(bot, w.0, w.1, w.2)) {
            continue; // never pour lava into a leaky cup
        }

        // 4. Pour LAVA into the cup.
        walk_to_xz(bot, pos.0 as f64 + 0.5, stand_z as f64 + 0.5, 0.2, 40).await;
        select_item(bot, "lava_bucket").await.ok();
        reliable_use(bot, vec3(pos.0 as f64 + 0.5, pos.1 as f64 + 0.2, pos.2 as f64 + 0.5)).await;
        bot.wait_ticks(8).await.ok();
        cdbg(&format!("cast {pos:?} a{_attempt}: after_lava cup_block={}", name_at(bot, pos.0, pos.1, pos.2)));

        // 5. Pour WATER into the block directly ABOVE the lava → obsidian.
        //    Packet-sniffing showed the bucket raycast falls THROUGH the open air block
        //    above the lava and places water down IN the cup (replacing the lava) —
        //    because nothing solid sits at (pos.y+1) to place against. So first build a
        //    "splash wall" just NORTH of the target at pos.y+1, then aim the water
        //    use_item at THAT wall: the ray hits its near face and the bucket drops the
        //    water source in the block in front of it — exactly above the lava. (Stay
        //    at feet pos.y+1; use_item, not use_item_on, is the only thing that pours a
        //    bucket.)
        let splash = (pos.0, pos.1 + 1, pos.2 - 1); // wall north of `above`, same level
        ensure_solid(bot, splash, 0).await;
        walk_to_xz(bot, pos.0 as f64 + 0.5, stand_z as f64 + 0.5, 0.3, 30).await;
        select_item(bot, "water_bucket").await.ok();
        reliable_use(bot, vec3(splash.0 as f64 + 0.5, splash.1 as f64 + 0.5, splash.2 as f64 + 0.9)).await;
        bot.wait_ticks(6).await.ok();
        cdbg(&format!(
            "cast {pos:?} a{_attempt}: after_water cup={} above={} splash={} wbkt={}",
            name_at(bot, pos.0, pos.1, pos.2),
            name_at(bot, above.0, above.1, above.2),
            name_at(bot, splash.0, splash.1, splash.2),
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
        cdbg(&format!(
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
    let deadline = Instant::now() + Duration::from_secs(360);

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
        cdbg(&format!("prepare: at ({:.0},{:.0},{:.0}) lava={lava:?}", p.x, p.y, p.z));
    }
    if lava.is_none() {
        for _ in 0..8 {
            if Instant::now() > deadline {
                break;
            }
            if feet_y(bot) > 13 {
                // Descend toward y~12 using the ore-miner's dig-down behaviour.
                let _ = crate::tasks::mining::mine_ore(bot, "iron", 9999, mem).await; // descends + mines; bails at deadline
            }
            lava = find_fluid(bot, "lava", 16);
            if lava.is_some() {
                break;
            }
        }
    }
    let Some(lava) = lava else {
        return None;
    };
    mem.log("cast", "lava", &format!("{},{},{}", lava.0, lava.1, lava.2));

    // 2. Anchor the frame a fixed gap EAST (+X) of the lava so the frame (which
    //    extends +X) never overlaps the pool — the old "stand on the bot's side"
    //    anchored the east column INSIDE the lava. The bot stands east and refills
    //    its lava bucket by walking west across the cleared gap.
    bot.set_control_state("sneak", false);
    let stand = (lava.0 + 6, lava.1 + 1, lava.2);
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
        let pos = (bx, by, bz - 2);
        cdbg(&format!("CAST_ONE casting {pos:?} from ({bx},{by},{bz})"));
        let ok = cast_obsidian_at(bot, pos, by, None).await;
        return if ok {
            success(format!("CAST_ONE ok — obsidian at {pos:?}"))
        } else {
            failure(format!("CAST_ONE failed at {pos:?}"))
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

    let mut cast = 0;
    // Bottom row first (its water bowl occupies an inner-fill cell), then inner fill,
    // then the rest bottom-up.
    for &pos in frame.iter().filter(|p| p.1 == by) {
        if cast_obsidian_at(bot, pos, by, lava_pool).await {
            cast += 1;
        } else {
            return failure(format!("cast {cast}/10 — stuck at {},{},{}", pos.0, pos.1, pos.2));
        }
    }
    build_inner_fill(bot, bx, by, bz).await;
    for &pos in frame.iter().filter(|p| p.1 > by) {
        if cast_obsidian_at(bot, pos, by, lava_pool).await {
            cast += 1;
        } else {
            return failure(format!("cast {cast}/10 — stuck at {},{},{}", pos.0, pos.1, pos.2));
        }
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
