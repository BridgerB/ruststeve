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
    let (Some(body), Some(dim)) = (w.body, w.dimension.as_ref()) else {
        return Vec::new();
    };
    let f = [body.pos[0], body.pos[1], body.pos[2]];
    let (cx, cz) = ((f[0].floor() as i32) >> 4, (f[2].floor() as i32) >> 4);
    let mut out: Vec<(f64, (i32, i32, i32))> = Vec::new();
    for dx in -chunks..=chunks {
        for dz in -chunks..=chunks {
            for s in w.memory.in_column(&dim.name, cx + dx, cz + dz) {
                if data::block_of_state(s.state) == Some(data::BlockKind::NetherBricks) {
                    let c = s.pos;
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
    let Some(body) = w.body else {
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
    /// Holds an iron pickaxe and a bucket (issue 13's iron kit: the pickaxe for obsidian, the
    /// bucket for the cast, clojurecraft's kit).
    IronKit,
    /// In the Nether (issue 15's entry).
    Nether,
    /// Has seen nether bricks with an exposed face (issue 15: "reach a fortress").
    Fortress,
    /// Holds 7 blaze rods (issue 16: "reach 7 rods").
    Rods7,
    /// Holds 12 ender pearls (issue 18: "reach 12 pearls").
    Pearls12,
    /// In the End (issue 20).
    End,
}

impl Stage {
    /// Every stage, lowest first.
    pub const ALL: [Stage; 10] = [
        Stage::InWorld,
        Stage::Log,
        Stage::Table,
        Stage::StonePickaxe,
        Stage::IronKit,
        Stage::Nether,
        Stage::Fortress,
        Stage::Rods7,
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
            Stage::IronKit => "iron_kit",
            Stage::Nether => "nether",
            Stage::Fortress => "fortress",
            Stage::Rods7 => "rods_7",
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
            Stage::IronKit => has(ItemKind::IronPickaxe, 1) && has(ItemKind::Bucket, 1),
            Stage::Nether => dim == Some("minecraft:the_nether"),
            Stage::Fortress => at_fortress(w),
            Stage::Rods7 => has(ItemKind::BlazeRod, 7),
            Stage::Pearls12 => has(ItemKind::EnderPearl, 12),
            Stage::End => dim == Some("minecraft:the_end"),
        }
    }

    /// The highest stage the world holds now: the ladder read from the top. A run's stage
    /// reached is the highest this has ever been (items get spent), kept by the world.
    pub fn highest(w: &World) -> Option<Stage> {
        Stage::ALL.into_iter().rev().find(|s| s.holds(w))
    }
}
