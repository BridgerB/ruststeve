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
    /// In the End (issue 20).
    End,
}

impl Stage {
    /// Every stage, lowest first.
    pub const ALL: [Stage; 17] = [
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
        Stage::End,
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
            Stage::End => "end",
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
            Stage::End => dim == Some("minecraft:the_end"),
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
            .filter(|s| scans || !matches!(s, Stage::Fortress | Stage::Spawner))
            .find(|s| s.holds(w))
    }
}
