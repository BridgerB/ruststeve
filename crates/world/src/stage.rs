//! The stage ladder (brain/ruststeve/design/race-and-results.md): how far a run got, read from
//! the world the bot holds, never from what it meant to do. The order is the plan's
//! (brain/ruststeve/roadmap/first-twenty-issues.md); a stage the world cannot detect yet (a lit
//! portal, the portal room, the dragon) is added by the issue that needs it, at its place in
//! the order (recorded files carry stages by name, so a new one never moves an old one).

use data::ItemKind;
use data::generated::tags::item_tag;

use crate::World;

/// Within this of a nether-brick cell seen exposed, the bot is at a fortress (issue 15): seeing
/// one counted at first, and 112 of 112 bots of the first nether race met it from where they
/// stood, open bridges in view 180 blocks off (docs/problems/fortress.md wall 1).
pub const FORTRESS_NEAR: f64 = 6.0;

/// The nether-brick cells on record in the dimension the bot is in, within `chunks` chunk columns
/// of the feet, nearest first.
pub fn bricks_near(w: &World, chunks: i32) -> Vec<(i32, i32, i32)> {
    kind_near(w, data::BlockKind::NetherBricks, chunks)
}

/// World ticks between two looks at the stages (a second).
pub const STAGE_EVERY_TICKS: u64 = 20;

/// The cells of `kind` on record in the dimension the bot is in, within `chunks` chunk columns of
/// the feet, nearest first (memory's watched kinds only).
pub fn kind_near(w: &World, kind: data::BlockKind, chunks: i32) -> Vec<(i32, i32, i32)> {
    let (Some(body), Some(dim)) = (w.body, w.dimension.as_ref()) else {
        return Vec::new();
    };
    let f = [body.pos[0], body.pos[1], body.pos[2]];
    let (cx, cz) = ((f[0].floor() as i32) >> 4, (f[2].floor() as i32) >> 4);
    let mut out: Vec<(f64, (i32, i32, i32))> = Vec::new();
    for dx in -chunks..=chunks {
        for dz in -chunks..=chunks {
            let cells = w
                .memory
                .indexed_in_column(&dim.name, cx + dx, cz + dz, kind)
                .unwrap_or_else(|| {
                    w.memory
                        .in_column(&dim.name, cx + dx, cz + dz)
                        .into_iter()
                        .filter(|s| data::block_of_state(s.state) == Some(kind))
                        .map(|s| s.pos)
                        .collect()
                });
            for c in cells {
                {
                    let d = (0..3)
                        .map(|i| {
                            let v = [c.0, c.1, c.2][i];
                            (f64::from(v) + 0.5 - f[i]).powi(2)
                        })
                        .sum::<f64>()
                        .sqrt();
                    out.push((d, c));
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    out.into_iter().map(|(_, c)| c).collect()
}

/// Whether the feet are within `FORTRESS_NEAR` of a nether-brick cell on record.
pub fn at_fortress(w: &World) -> bool {
    let Some(body) = w.body.filter(|_| in_nether(w)) else {
        return false;
    };
    bricks_near(w, 1).first().is_some_and(|c| {
        let d = [
            f64::from(c.0) + 0.5,
            f64::from(c.1) + 0.5,
            f64::from(c.2) + 0.5,
        ];
        (0..3)
            .map(|i| (d[i] - body.pos[i]).powi(2))
            .sum::<f64>()
            .sqrt()
            <= FORTRESS_NEAR
    })
}

/// Within this of a spawner cell seen exposed, the bot is at the spawner (issue 16b): clojurecraft's
/// `camp-range` 8; a spawner wakes for a player within 16.
pub const SPAWNER_NEAR: f64 = 8.0;

/// Within this of an enderman that ticks, the bot has got to endermen (issue 17's done
/// condition, #22: "within 32 blocks"). At the race's simulation distance 3 an enderman this
/// near always ticks (its chunk is at most 2 from the player's); the ticking test still guards
/// a server run at a smaller distance.
pub const ENDERMAN_NEAR: f64 = 32.0;

/// The nearest live enderman that ticks within `ENDERMAN_NEAR` of the feet: its id and distance
/// (feet to feet).
pub fn enderman_near(w: &World) -> Option<(i32, f64)> {
    let body = w.body?;
    let feet = [body.pos[0], body.pos[1], body.pos[2]];
    w.entities
        .iter()
        .filter(|(_, e)| !e.dead && e.kind == data::EntityKind::Enderman)
        .map(|(id, e)| {
            let d = (0..3)
                .map(|i| (e.pos[i] - feet[i]).powi(2))
                .sum::<f64>()
                .sqrt();
            (*id, d, e.pos)
        })
        .filter(|(_, d, pos)| *d <= ENDERMAN_NEAR && crate::ticks(w, *pos))
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map(|(id, d, _)| (id, d))
}

/// The biome of the `forest` stage, as `World::feet_biome` names it.
pub const FOREST_BIOME: &str = "warped_forest";
/// Pearls of the `pearls_6` checkpoint: half of issue 18's twelve.
pub const PEARLS_HALF: u32 = 6;
/// Pearls of the `pearls_9` checkpoint (issue 18, 2026-11-25): a bot at 9 pearls whose supply ran
/// out (race 45602491038 lane 6) had no stage snapshot past 6 to resume from.
pub const PEARLS_THREE_QUARTERS: u32 = 9;
/// Pearls of the `pearls_12` stage (issue 18's done condition; issue 19's twelve eyes).
pub const PEARLS_FULL: u32 = 12;

/// Eyes and pearls together of the `overworld` checkpoint (issue 19): the twelve the End portal
/// may need (12 frames, every one empty 28% of the time), as eyes or pearls still to be made into
/// eyes.
pub const EYES_FULL: i64 = 12;
/// Within this (horizontal) of an `end_portal_frame` seen exposed, the stronghold is found (issue
/// 19's `stronghold` checkpoint): the race's view distance, 8 chunks (`config/race.toml`), is
/// as far as chunk data shows a frame.
pub const FRAME_NEAR: f64 = 128.0;
/// Chunk columns of memory looked through for frames: `FRAME_NEAR` / 16.
pub const FRAME_CHUNKS: i32 = 8;
/// The portal room's box about the frame ring's centre: within this along both axes the feet are
/// inside the room whichever way it faces. The piece is 11 by 16 with its walls (the 26.1.2 jar,
/// probe 45685968956); the race world's room (probe 45689842509: ring x 1626 to 1630, z -36 to
/// -32, frames at y 19, walls at z -39 and -29 and x 1623 and 1638, the door at x 1638) has an
/// inside 4.5 from the centre on its short axis and from -4.5 to +9.5 on its long one, the door's
/// end. The 8 first proposed reached through the west wall into a corridor at x 1621 to 1622
/// (air at y 19 to 20), so the box is the short axis's both ways.
pub const ROOM_HALF: f64 = 4.5;
/// The room's frames lie at piece y 3, its floor at y 0 (feet y 1), its stair landing higher:
/// feet y from the frame y - 2 to the frame y + 2 (the same template).
pub const ROOM_BELOW: i32 = 2;
pub const ROOM_ABOVE: i32 = 2;

/// In the overworld.
pub fn in_overworld(w: &World) -> bool {
    w.dimension
        .as_ref()
        .is_some_and(|d| d.name == "minecraft:overworld")
}

/// The `end_portal_frame` cells on record within `FRAME_NEAR` (horizontal) of the feet, in the
/// overworld, nearest first.
pub fn frames_near(w: &World) -> Vec<(i32, i32, i32)> {
    let Some(body) = w.body.filter(|_| in_overworld(w)) else {
        return Vec::new();
    };
    kind_near(w, data::BlockKind::EndPortalFrame, FRAME_CHUNKS)
        .into_iter()
        .filter(|c| {
            (f64::from(c.0) + 0.5 - body.pos[0]).hypot(f64::from(c.2) + 0.5 - body.pos[2])
                <= FRAME_NEAR
        })
        .collect()
}

/// The frame ring nearest the feet, from the frames on record within 16 of the nearest one: the
/// centre of their box (x, z) and their y. A ring is 12 frames on a 5 by 5 square less its
/// corners, so the box's middle is the ring's centre once both sides are seen.
pub fn frame_ring(w: &World) -> Option<([f64; 2], i32)> {
    let frames = frames_near(w);
    let first = *frames.first()?;
    let ring: Vec<_> = frames
        .into_iter()
        .filter(|c| (c.0 - first.0).abs() <= 16 && (c.2 - first.2).abs() <= 16 && c.1 == first.1)
        .collect();
    let (x0, x1) = ring
        .iter()
        .fold((i32::MAX, i32::MIN), |(a, b), c| (a.min(c.0), b.max(c.0)));
    let (z0, z1) = ring
        .iter()
        .fold((i32::MAX, i32::MIN), |(a, b), c| (a.min(c.2), b.max(c.2)));
    Some((
        [
            (f64::from(x0) + f64::from(x1) + 1.0) / 2.0,
            (f64::from(z0) + f64::from(z1) + 1.0) / 2.0,
        ],
        first.1,
    ))
}

/// Whether the feet stand in the portal room: an air cell within `ROOM_HALF` of the ring's
/// centre along each axis, at a y the room holds (`ROOM_BELOW`, `ROOM_ABOVE`).
pub fn in_portal_room(w: &World) -> bool {
    let (Some(body), Some((centre, fy))) = (w.body, frame_ring(w)) else {
        return false;
    };
    let feet = crate::BlockPos::new(
        body.pos[0].floor() as i32,
        body.pos[1].floor() as i32,
        body.pos[2].floor() as i32,
    );
    let half = ROOM_HALF;
    (body.pos[0] - centre[0]).abs() <= half
        && (body.pos[2] - centre[1]).abs() <= half
        && (fy - ROOM_BELOW..=fy + ROOM_ABOVE).contains(&feet.y)
        && matches!(
            w.block_at(feet),
            Some(data::BlockKind::Air | data::BlockKind::CaveAir)
        )
}

/// In the Nether: a fortress and its blaze spawner are nowhere else. A dungeon's monster spawner
/// at y -21 in the overworld made a stone-tool bot `spawner`, and so every stage under it
/// (race 44804466844; blaze wall 4).
pub fn in_nether(w: &World) -> bool {
    w.dimension
        .as_ref()
        .is_some_and(|d| d.name == "minecraft:the_nether")
}

/// Whether the feet are within `SPAWNER_NEAR` of a spawner cell on record.
pub fn at_spawner(w: &World) -> bool {
    let Some(body) = w.body.filter(|_| in_nether(w)) else {
        return false;
    };
    kind_near(w, data::BlockKind::Spawner, 1)
        .first()
        .is_some_and(|c| {
            let d = [
                f64::from(c.0) + 0.5,
                f64::from(c.1) + 0.5,
                f64::from(c.2) + 0.5,
            ];
            (0..3)
                .map(|i| (d[i] - body.pos[i]).powi(2))
                .sum::<f64>()
                .sqrt()
                <= SPAWNER_NEAR
        })
}

/// Logs a ready bot holds: a table and sticks for a dozen stone pickaxes (a log is four planks,
/// a pickaxe two sticks; blaze walls 6 to 8 were bots with no wood under 40 blocks of stone).
pub const READY_LOGS: i64 = 4;
/// Hunger points of safe food a ready bot holds: `[survival] food_reserve` in goals.toml.
pub const READY_FOOD: i64 = 32;
/// How far over the head the sky is checked for.
pub const SKY_CHECK: i32 = 48;
/// Food never counted: `[survival] unsafe_foods` in goals.toml (what poisons, starves or is worth
/// more than a meal).
const UNSAFE_FOOD: [ItemKind; 9] = [
    ItemKind::RottenFlesh,
    ItemKind::SpiderEye,
    ItemKind::Pufferfish,
    ItemKind::PoisonousPotato,
    ItemKind::Chicken,
    ItemKind::ChorusFruit,
    ItemKind::SuspiciousStew,
    ItemKind::GoldenApple,
    ItemKind::EnchantedGoldenApple,
];

/// How many items of an item tag are held.
fn tagged(w: &World, tag: &str) -> i64 {
    item_tag(tag).map_or(0, |items| items.iter().map(|k| w.inventory.count(*k)).sum())
}

/// Hunger points of safe food held.
pub fn safe_food(w: &World) -> i64 {
    w.inventory
        .slots
        .values()
        .filter(|(k, _)| !UNSAFE_FOOD.contains(k))
        .filter_map(|(k, n)| {
            data::generated::item_props::food_of(*k).map(|f| i64::from(f.nutrition) * i64::from(*n))
        })
        .sum()
}

/// Nothing but air (or a tree) in the loaded column over the head for `SKY_CHECK` blocks: the
/// bot is above ground. Each cell read is over the bot's own column, seen from below.
pub fn open_sky(w: &World) -> bool {
    let Some(body) = w.body else {
        return false;
    };
    let (x, y, z) = (
        body.pos[0].floor() as i32,
        body.pos[1].floor() as i32,
        body.pos[2].floor() as i32,
    );
    let tree = |k: data::BlockKind| {
        ["leaves", "logs"]
            .iter()
            .filter_map(|t| data::generated::tags::block_tag(t))
            .any(|ks| ks.contains(&k))
    };
    ((y + 2)..(y + 2 + SKY_CHECK)).all(|yy| {
        match w.block_at(crate::BlockPos::new(x, yy, z)) {
            Some(data::BlockKind::Air | data::BlockKind::CaveAir) => true,
            Some(k) => tree(k),
            // Over the top of the loaded column: nothing there.
            None => yy > 319,
        }
    })
}

// >>> E1: issue 20's stages (the numbers are `config/end.toml`'s; a test in
// crates/world/tests/stages.rs holds the two equal).

/// `armed`: pickaxes (stone or better) held, at least (`[kit] pickaxes`).
pub const ARMED_PICKAXES: usize = 2;
/// `armed`: uses left over those pickaxes (`[kit] pickaxe_uses`: two fresh stone ones, 262).
pub const ARMED_PICKAXE_USES: i32 = 260;
/// `armed`: cooked food items (`[kit] cooked`).
pub const ARMED_COOKED: i64 = 24;
/// `armed`: building blocks (`[kit] blocks`).
pub const ARMED_BLOCKS: i64 = 128;
// >>> E8 (issue 20, the bow line)
/// `bow_armed`: arrows held with the bow (`[end_kit] arrows` wants 32; 8 is the least: one
/// for each open crystal, parity e07's two shots both hit; race 46067009450's bots made 8 to 12
/// in a morning of chickens).
pub const BOW_ARROWS: i64 = 8;
// <<< E8
/// Frames in a ring: the empty ones are these less those on record with an eye.
pub const RING_FRAMES: i64 = 12;
/// The fountain's column centre (the podium at 0, 0: EndPodiumFeature, the 26.1.2 jar).
pub const FOUNTAIN: [f64; 2] = [0.5, 0.5];
/// `island`: the feet on end stone within this (horizontal) of the fountain (`[island] near`).
pub const ISLAND_NEAR: f64 = 30.0;
/// The feet within this of the fountain: every spike (42 off on a ring, probe
/// 45863427332) is within 102, inside the entity tracking distance (view distance 8 less one
/// chunk, 112): every crystal still alive has been sent (`[crystals] near`).
pub const CRYSTALS_NEAR: f64 = 60.0;
/// `crystals_5`: crystals left alive, at most (`[crystals] left_5`; issue 20 part 2, Bridger
/// 2026-11-27: race 45994686575 had three bots at 1 left and one at 2, and none saved).
pub const CRYSTALS_LEFT_5: usize = 5;
/// `crystals_1`: crystals left alive, at most (`[crystals] left_1`).
pub const CRYSTALS_LEFT_1: usize = 1;
/// `grounded`: full cells beside the block under the feet, at least (`[crystals]
/// ground_sides`: 3 of 4, so a stand beside a spike, 1, is not ground).
pub const GROUND_SIDES: usize = 3;
/// The ten spikes' centre columns, this seed (probe 45863427332; `[[spike]]` in
/// config/end.toml). Geography, Bridger's ruling: the End's layout is fixed by the seed.
pub const SPIKES: [[i32; 2]; 10] = [
    [12, 39],
    [-34, 24],
    [42, 0],
    [-34, -25],
    [-42, -1],
    [33, -25],
    [33, 24],
    [-13, -40],
    [-13, 39],
    [12, -40],
];

/// The pickaxes (stone or better) held `armed` counts.
const ARMED_PICKAXE_KINDS: [ItemKind; 4] = [
    ItemKind::StonePickaxe,
    ItemKind::IronPickaxe,
    ItemKind::DiamondPickaxe,
    ItemKind::NetheritePickaxe,
];
/// The cooked food `armed` counts.
pub const COOKED: [ItemKind; 9] = [
    ItemKind::CookedBeef,
    ItemKind::CookedPorkchop,
    ItemKind::CookedMutton,
    ItemKind::CookedChicken,
    ItemKind::CookedRabbit,
    ItemKind::CookedCod,
    ItemKind::CookedSalmon,
    ItemKind::Bread,
    ItemKind::BakedPotato,
];
/// The blocks `armed` counts: the bridge blocks (`observe::FILLER`) and end stone.
pub const ARMED_BLOCK_KINDS: [ItemKind; 11] = [
    ItemKind::Cobblestone,
    ItemKind::CobbledDeepslate,
    ItemKind::Netherrack,
    ItemKind::Granite,
    ItemKind::Diorite,
    ItemKind::Andesite,
    ItemKind::Tuff,
    ItemKind::Dirt,
    ItemKind::Basalt,
    ItemKind::Blackstone,
    ItemKind::EndStone,
];

/// In the End.
pub fn in_end(w: &World) -> bool {
    w.dimension
        .as_ref()
        .is_some_and(|d| d.name == "minecraft:the_end")
}

/// The frames on record within `FRAME_NEAR` that hold an eye (`memory::frame_has_eye`).
pub fn frames_with_eye(w: &World) -> i64 {
    let Some(dim) = w.dimension.as_ref() else {
        return 0;
    };
    frames_near(w)
        .into_iter()
        .filter(|c| {
            w.memory
                .latest(&dim.name, *c)
                .is_some_and(|s| memory::frame_has_eye(s.state))
        })
        .count() as i64
}

/// The ring's empty frames: `RING_FRAMES` less those on record with an eye (a frame never seen
/// counts as empty).
pub fn empty_frames(w: &World) -> i64 {
    (RING_FRAMES - frames_with_eye(w)).max(0)
}

/// Uses left over the pickaxes `armed` counts, and how many.
pub fn armed_pickaxes(w: &World) -> (usize, i32) {
    let picks: Vec<i32> = w
        .inventory
        .slots
        .iter()
        .filter(|(_, (k, _))| ARMED_PICKAXE_KINDS.contains(k))
        .filter_map(|(slot, _)| w.inventory.uses_left(*slot))
        .collect();
    (picks.len(), picks.iter().sum())
}

/// The kit of `armed`: an iron sword, `ARMED_PICKAXES` pickaxes with `ARMED_PICKAXE_USES`,
/// `ARMED_COOKED` cooked food, eyes for every empty frame, `ARMED_BLOCKS` blocks, a water bucket.
pub fn armed_kit(w: &World) -> bool {
    let count = |ks: &[ItemKind]| ks.iter().map(|k| w.inventory.count(*k)).sum::<i64>();
    let (picks, uses) = armed_pickaxes(w);
    w.inventory.count(ItemKind::IronSword) >= 1
        && picks >= ARMED_PICKAXES
        && uses >= ARMED_PICKAXE_USES
        && count(&COOKED) >= ARMED_COOKED
        && w.inventory.count(ItemKind::EnderEye) >= empty_frames(w)
        && count(&ARMED_BLOCK_KINDS) >= ARMED_BLOCKS
        && w.inventory.count(ItemKind::WaterBucket) >= 1
}

/// The feet's horizontal distance to the fountain.
pub fn from_fountain(w: &World) -> Option<f64> {
    w.body
        .map(|b| (b.pos[0] - FOUNTAIN[0]).hypot(b.pos[2] - FOUNTAIN[1]))
}

/// The feet on end stone within `ISLAND_NEAR` of the fountain, in the End.
pub fn on_island(w: &World) -> bool {
    let Some(body) = w.body.filter(|_| in_end(w)) else {
        return false;
    };
    let under = crate::BlockPos::new(
        body.pos[0].floor() as i32,
        (body.pos[1] - 0.01).floor() as i32,
        body.pos[2].floor() as i32,
    );
    from_fountain(w).is_some_and(|d| d <= ISLAND_NEAR)
        && w.block_at(under) == Some(data::BlockKind::EndStone)
}

/// End crystals the world holds alive.
pub fn crystals_alive(w: &World) -> usize {
    w.entities
        .values()
        .filter(|e| !e.dead && e.kind == data::EntityKind::EndCrystal)
        .count()
}

/// The crystals left, in the End: the ten spikes less those whose crystal was seen destroyed
/// (memory's record: a removal with its blast at its spot). A crystal untracked (the bot far
/// off, or the server re-sending them on a resume: race 46044026128, every lane "at 5 left" on
/// tick 40) is not gone. `None` outside the End.
pub fn crystals_left(w: &World) -> Option<usize> {
    in_end(w).then(|| {
        SPIKES
            .len()
            .saturating_sub(w.memory.crystals_destroyed().len())
    })
}

/// The feet on the ground, not on a pillar or a spike's top: on ground, on end stone (a spike's
/// top is obsidian and bedrock), and at least `GROUND_SIDES` of the four cells beside the block
/// under the feet loaded and full. A 1x1 pillar has none of them, a stand beside a spike one,
/// the island's surface four. A checkpoint
/// taken on a stand resumes stranded: race 46059621531, from `crystals_5` taken at the 5th
/// strike on (-33, 79, 27), 7 of 10 lanes "stranded on a spike's top" and dead in 3,460 ticks.
pub fn grounded(w: &World) -> bool {
    let Some(body) = w.body.filter(|b| b.on_ground) else {
        return false;
    };
    let (x, y, z) = (
        body.pos[0].floor() as i32,
        (body.pos[1] - 0.01).floor() as i32,
        body.pos[2].floor() as i32,
    );
    let full = |dx: i32, dz: i32| {
        w.state_at(crate::BlockPos::new(x + dx, y, z + dz))
            .is_some()
            && w.solid(x + dx, y, z + dz) == Some(1.0)
    };
    w.block_at(crate::BlockPos::new(x, y, z)) == Some(data::BlockKind::EndStone)
        && [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .iter()
            .filter(|(dx, dz)| full(*dx, *dz))
            .count()
            >= GROUND_SIDES
}

/// Every spike's crystal seen destroyed, in the End (`crystals_left`). It was once "none alive
/// near the fountain with every spike loaded", which a resume's re-send of the crystals could
/// satisfy for a tick.
pub fn crystals_gone(w: &World) -> bool {
    crystals_left(w) == Some(0)
}
// <<< E1

/// A stage, lowest first. `Ord` is the ladder.
#[derive(
    serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum Stage {
    /// In play with the body placed (issues 03 to 06).
    InWorld,
    /// Holds a log (issue 09's goal: a log reached).
    Log,
    /// Holds a crafting table (issue 10: "hold a crafting table").
    Table,
    /// Holds a stone pickaxe (issue 11).
    StonePickaxe,
    /// Holds (or wears) an iron helmet, chestplate and leggings and an iron sword (issue 16's gear,
    /// made before the iron kit by the race aim; a geared bot is snapshotted, so the chain resumes
    /// from it, not from spawn: 4 of 84 spawn bots geared in 3 game hours and got no further).
    Gear,
    /// Ready for the portal, above ground (issue 16, Bridger 2026-11-22: "a snapshot that is super
    /// clean"): the gear, a water bucket, an empty bucket, flint and steel, a pickaxe,
    /// `READY_LOGS` logs and `READY_FOOD` hunger points of safe food, under the open sky. Every
    /// gear snapshot before it was a bot deep in a cave with a stick or two (blaze walls 5 to 16).
    Ready,
    /// Holds an iron pickaxe and a bucket (issue 13's iron kit: the pickaxe for obsidian, the
    /// bucket for the cast, clojurecraft's kit).
    IronKit,
    /// In the Nether (issue 15's entry).
    Nether,
    /// The feet within `FORTRESS_NEAR` of nether bricks seen exposed (issue 15: "reach a fortress").
    Fortress,
    /// The feet within `SPAWNER_NEAR` of a blaze spawner seen exposed (issue 16b: the fight attempts
    /// start in the spawner room, not at the fortress gate).
    Spawner,
    /// Holds 7 blaze rods (issue 16: "reach 7 rods").
    Rods7,
    /// Holds the 7 rods and the feet within `ENDERMAN_NEAR` of an enderman that ticks (issue 17:
    /// "getting to endermen"; one beyond the simulation distance is frozen and does not count).
    /// The rods are part of it so the ladder stays a ladder: an enderman passing a bot at the
    /// spawner before rod 7 (race 45358767468 lane 1, one at 31 blocks) is not this stage.
    Endermen,
    /// Holds the 7 rods with the feet in a warped forest (`World::feet_biome`; issue 18's
    /// "checkpoint start: forest", Bridger 2026-11-24): the only enderman-dense biome (the 26.1.2
    /// jar: its one monster spawn, 17a), and the base whose stems are wood. The fight races
    /// start here.
    Forest,
    /// Holds the 7 rods and `PEARLS_HALF` pearls (issue 18's halfway checkpoint, Bridger
    /// 2026-11-24): a long hunt is not replayed from its first kill after a late failure.
    Pearls6,
    /// Holds the 7 rods and `PEARLS_THREE_QUARTERS` pearls (issue 18, 2026-11-25): a third
    /// checkpoint, so a hunt that stalls past 6 is resumed from where it got to.
    Pearls9,
    /// Holds the 7 rods and 12 ender pearls (issue 18: "reach 12 pearls"). The rods are part of
    /// it so the ladder stays a ladder (issue 19 needs six of them for the eyes).
    Pearls12,
    /// In the overworld holding `EYES_FULL` eyes of ender and pearls together (issue 19's first
    /// checkpoint, Bridger 2026-11-25): home through the bot's own portal with the eyes, so the
    /// overland trip never replays the Nether.
    Overworld,
    /// In the overworld with an `end_portal_frame` on record within `FRAME_NEAR` (issue 19's
    /// second checkpoint): the portal room is located; what is left is the dig.
    Stronghold,
    /// The feet in the portal room (`in_portal_room`): issue 19's done condition.
    PortalRoom,
    /// In the overworld with the End's kit (`armed_kit`; issue 20's first checkpoint, Bridger
    /// 2026-11-26): the overworld prep is never replayed.
    Armed,
    // >>> E8 (issue 20, the bow line, docs/problems/end-bow.md)
    /// `armed` reached, then a bow and `BOW_ARROWS` arrows in the overworld: the bow line's first
    /// checkpoint. Accreted between `armed` and `end` so `Ord` stays the ladder.
    BowArmed,
    // <<< E8
    /// In the End (issue 20).
    End,
    /// The feet on the main island's end stone within `ISLAND_NEAR` of the fountain (issue 20).
    Island,
    /// `crystals_left` at most `CRYSTALS_LEFT_5` (issue 20 part 2's checkpoint, Bridger
    /// 2026-11-27): a bot half through the spikes is saved, not replayed from the island. Only
    /// with the feet on the ground (`grounded`), so the snapshot is not on a stand. Accreted
    /// between `island` and `crystals` so `Ord` stays the ladder (stages go by name in files).
    Crystals5,
    /// `crystals_left` at most `CRYSTALS_LEFT_1` (issue 20 part 2): the last spike's attempts
    /// resume from it.
    Crystals1,
    /// In the End with every spike's crystal seen destroyed (`crystals_gone`; issue 20).
    Crystals,
    /// The credits (`win_game`) with the dragon seen dead before them (issue 20's done
    /// condition).
    Win,
}

impl Stage {
    /// Every stage, lowest first.
    pub const ALL: [Stage; 27] = [
        Stage::InWorld,
        Stage::Log,
        Stage::Table,
        Stage::StonePickaxe,
        Stage::Gear,
        Stage::Ready,
        Stage::IronKit,
        Stage::Nether,
        Stage::Fortress,
        Stage::Spawner,
        Stage::Rods7,
        Stage::Endermen,
        Stage::Forest,
        Stage::Pearls6,
        Stage::Pearls9,
        Stage::Pearls12,
        Stage::Overworld,
        Stage::Stronghold,
        Stage::PortalRoom,
        Stage::Armed,
        Stage::BowArmed,
        Stage::End,
        Stage::Island,
        Stage::Crystals5,
        Stage::Crystals1,
        Stage::Crystals,
        Stage::Win,
    ];

    /// The name results files and the insight store use.
    pub fn name(self) -> &'static str {
        match self {
            Stage::InWorld => "in_world",
            Stage::Log => "log",
            Stage::Table => "table",
            Stage::StonePickaxe => "stone_pickaxe",
            Stage::Gear => "gear",
            Stage::Ready => "ready",
            Stage::IronKit => "iron_kit",
            Stage::Nether => "nether",
            Stage::Fortress => "fortress",
            Stage::Spawner => "spawner",
            Stage::Rods7 => "rods_7",
            Stage::Endermen => "endermen",
            Stage::Forest => "forest",
            Stage::Pearls6 => "pearls_6",
            Stage::Pearls9 => "pearls_9",
            Stage::Pearls12 => "pearls_12",
            Stage::Overworld => "overworld",
            Stage::Stronghold => "stronghold",
            Stage::PortalRoom => "portal_room",
            Stage::Armed => "armed",
            Stage::BowArmed => "bow_armed",
            Stage::End => "end",
            Stage::Island => "island",
            Stage::Crystals5 => "crystals_5",
            Stage::Crystals1 => "crystals_1",
            Stage::Crystals => "crystals",
            Stage::Win => "win",
        }
    }

    pub fn from_name(name: &str) -> Option<Stage> {
        Stage::ALL.into_iter().find(|s| s.name() == name)
    }

    /// Whether the world holds this stage now.
    pub fn holds(self, w: &World) -> bool {
        let has = |item: ItemKind, n: i64| w.inventory.count(item) >= n;
        let dim = w.dimension.as_ref().map(|d| d.name.as_str());
        match self {
            Stage::InWorld => w.body.is_some(),
            Stage::Log => item_tag("logs")
                .is_some_and(|logs| logs.iter().any(|item| w.inventory.count(*item) > 0)),
            Stage::Table => has(ItemKind::CraftingTable, 1),
            Stage::StonePickaxe => has(ItemKind::StonePickaxe, 1),
            Stage::Gear => [
                ItemKind::IronHelmet,
                ItemKind::IronChestplate,
                ItemKind::IronLeggings,
                ItemKind::IronSword,
            ]
            .iter()
            .all(|k| has(*k, 1)),
            Stage::Ready => {
                Stage::Gear.holds(w)
                    && has(ItemKind::WaterBucket, 1)
                    && has(ItemKind::Bucket, 1)
                    && has(ItemKind::FlintAndSteel, 1)
                    && tagged(w, "pickaxes") >= 1
                    && tagged(w, "logs") >= READY_LOGS
                    && safe_food(w) >= READY_FOOD
                    && open_sky(w)
            }
            Stage::IronKit => has(ItemKind::IronPickaxe, 1) && has(ItemKind::Bucket, 1),
            Stage::Nether => dim == Some("minecraft:the_nether"),
            Stage::Fortress => at_fortress(w),
            Stage::Spawner => at_spawner(w),
            Stage::Rods7 => has(ItemKind::BlazeRod, 7),
            Stage::Endermen => has(ItemKind::BlazeRod, 7) && enderman_near(w).is_some(),
            Stage::Forest => has(ItemKind::BlazeRod, 7) && w.feet_biome() == Some(FOREST_BIOME),
            Stage::Pearls6 => {
                has(ItemKind::BlazeRod, 7) && has(ItemKind::EnderPearl, i64::from(PEARLS_HALF))
            }
            Stage::Pearls9 => {
                has(ItemKind::BlazeRod, 7)
                    && has(ItemKind::EnderPearl, i64::from(PEARLS_THREE_QUARTERS))
            }
            Stage::Pearls12 => {
                has(ItemKind::BlazeRod, 7) && has(ItemKind::EnderPearl, i64::from(PEARLS_FULL))
            }
            Stage::Overworld => {
                dim == Some("minecraft:overworld")
                    && w.inventory.count(ItemKind::EnderEye)
                        + w.inventory.count(ItemKind::EnderPearl)
                        >= EYES_FULL
            }
            Stage::Stronghold => !frames_near(w).is_empty(),
            Stage::PortalRoom => in_portal_room(w),
            Stage::Armed => dim == Some("minecraft:overworld") && armed_kit(w),
            Stage::BowArmed => {
                dim == Some("minecraft:overworld")
                    && has(ItemKind::Bow, 1)
                    && has(ItemKind::Arrow, BOW_ARROWS)
                    // `armed` reached, not held now: the bow phase eats into the kit's food
                    // (race 46072716511 lane 1 held a bow and 12 arrows and never read armed
                    // again).
                    && w.stage_reached.is_some_and(|s| s >= Stage::Armed)
            }
            Stage::End => dim == Some("minecraft:the_end"),
            Stage::Island => on_island(w),
            Stage::Crystals5 => {
                crystals_left(w).is_some_and(|n| n <= CRYSTALS_LEFT_5) && grounded(w)
            }
            Stage::Crystals1 => {
                crystals_left(w).is_some_and(|n| n <= CRYSTALS_LEFT_1) && grounded(w)
            }
            Stage::Crystals => crystals_gone(w),
            Stage::Win => w.won && w.dragon_dead.is_some(),
        }
    }

    /// The highest stage the world holds now: the ladder read from the top. A run's stage
    /// reached is the highest this has ever been (items get spent), kept by the world.
    pub fn highest(w: &World) -> Option<Stage> {
        Stage::ALL.into_iter().rev().find(|s| s.holds(w))
    }

    /// The highest stage that holds of those above `reached` (all of them when `None`): the
    /// stages at or under it are not looked at again, and the ones that scan the sightings of 25
    /// chunk columns (`scans`) only when `scans`.
    pub fn highest_above(w: &World, reached: Option<Stage>, scans: bool) -> Option<Stage> {
        Stage::ALL
            .into_iter()
            .rev()
            .take_while(|s| reached.is_none_or(|r| *s > r))
            .filter(|s| {
                scans
                    || !matches!(
                        s,
                        Stage::Fortress
                            | Stage::Spawner
                            | Stage::Stronghold
                            | Stage::PortalRoom
                            | Stage::Armed
                            | Stage::BowArmed
                    )
            })
            .find(|s| s.holds(w))
    }
}
