//! The parity table's data (brain/ruststeve/design/parity-table.md): a moment, the outcome
//! class a run of it ends in, and the comparison of real against sim, field by field.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Source;

/// One recorded moment (a row of `config/moments.toml`): a captured landing, where on it to
/// start, what to press and for how long.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Moment {
    pub id: String,
    /// The landing's index in `config/landings.toml`.
    pub landing: usize,
    /// Blocks from the landing's centre; the start stands on the surface there.
    pub dx: i32,
    pub dz: i32,
    /// A script name (`straight:90`, `jump:180`, `square`, `still`, `dig:30`,
    /// `craft:planks`).
    pub script: String,
    pub ticks: u64,
    /// Items given at the start, as `item count` (`oak_log 1`): `/give` in real, into the
    /// model's inventory in sim (issue 10b).
    #[serde(default)]
    pub give: Vec<String>,
    /// Blocks laid at the start, as `[block, dx, dy, dz]` from the start's feet cell: `setblock`
    /// by RCON in real, into the model's world in sim (issue 11a: stone under the feet, room for
    /// a table).
    #[serde(default)]
    pub set: Vec<(String, i32, i32, i32)>,
    /// Whether the real side gives water breathing (it does unless a row tests drowning; the
    /// model's air follows it) (issue 12a).
    #[serde(default = "yes")]
    pub breathe: bool,
    /// Effects given at the start, as `[effect, seconds, amplifier]` (`hunger`).
    #[serde(default)]
    pub effects: Vec<(String, u32, u32)>,
    /// Mobs placed at the start, as `[entity, dx, dy, dz]` from the feet cell: with no AI and no
    /// loot in real, and the same in the model.
    #[serde(default)]
    pub summon: Vec<(String, i32, i32, i32)>,
    /// Cells from the feet cell whose blocks join the outcome class (issue 14a: a source
    /// scooped, obsidian made, a portal lit).
    #[serde(default)]
    pub watch: Vec<(i32, i32, i32)>,
    /// Mobs placed with their AI, as `[entity, dx, dy, dz]` from the feet cell (issue 17:
    /// endermen frozen or ticking, a stare): `summon` with no loot and kept in real; the model's
    /// own brain in sim. Their outcome joins the class as `mobs`.
    #[serde(default)]
    pub mobs: Vec<(String, i32, i32, i32)>,
    /// The difficulty the row runs at (`normal`); none is the table's peaceful. Rows with one run
    /// after the others, the real server's difficulty set by RCON for them (an enderman is a
    /// monster: peaceful removes it at once).
    #[serde(default)]
    pub difficulty: Option<String>,
    /// Boxes filled at the start, before `set`, as `[block, dx1, dy1, dz1, dx2, dy2, dz2]` from the
    /// feet cell: `fill` by RCON in real, each cell into the model in sim (issue 18: a burrow and
    /// the arena round it).
    #[serde(default)]
    pub fill: Vec<(String, i32, i32, i32, i32, i32, i32)>,
    /// The placed mobs (`mobs`) keep their loot table (issue 18: a pearl may drop); by default
    /// they drop nothing.
    #[serde(default)]
    pub loot: bool,
    /// Items laid on the ground at the start, as `[item, dx, dy, dz]` from the feet cell, the
    /// cell's middle, no pickup delay (issue 18: a pearl at the mouth): `summon item` in real.
    #[serde(default)]
    pub items: Vec<(String, i32, i32, i32)>,
    /// The stronghold an eye of ender thrown in this row flies to, `[x, z]` in blocks, as RCON's
    /// `locate structure minecraft:stronghold` names it from the row's start (issue 19: the
    /// model's target, and the bearing the real eye's line is judged against).
    #[serde(default)]
    pub stronghold: Option<(i32, i32)>,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Moments {
    pub moment: Vec<Moment>,
}

/// A landing (a row of `config/landings.toml`): a column position on the race seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Landing {
    pub x: i32,
    pub z: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Landings {
    pub landing: Vec<Landing>,
}

impl Moments {
    pub fn read(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        toml::from_str(&text).map_err(|e| format!("{path}: {e}"))
    }
}

impl Landings {
    pub fn read(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        toml::from_str(&text).map_err(|e| format!("{path}: {e}"))
    }
}

/// How healthy the bot ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Health {
    Full,
    Hurt,
    Dead,
}

/// What became of a placed mob (`Moment::mobs`), as the bot's world saw it: the entity of its
/// kind nearest the place it was put.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MobOutcome {
    pub kind: String,
    /// The bot's world was told of it.
    pub seen: bool,
    /// It ended more than half a block from where it was put (it ticked: fell, walked,
    /// teleported).
    pub moved: bool,
    /// An enderman's creepy flag (entity data 17): it has a target.
    pub creepy: bool,
    /// An enderman's stared-at flag (entity data 18): a player's stare started its look goal
    /// (issue 18: the roof cuts the line from B).
    #[serde(default)]
    pub stared: bool,
    /// The cell its feet ended in (issue 18: it came to the mouth).
    #[serde(default)]
    pub cell: Option<[i32; 3]>,
}

/// The outcome class of a moment: what the table compares, field by field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    /// The block the feet ended in (floor of the position).
    pub cell: [i32; 3],
    pub on_ground: bool,
    pub in_water: bool,
    /// The server corrected the bot at least once after the script began.
    pub corrected: bool,
    pub health: Health,
    /// What the player holds at the end, by item name (window 0's slots 1-45 and the cursor),
    /// and how many items sit in the grid, its result and the cursor (issue 10b).
    #[serde(default)]
    pub inventory: BTreeMap<String, i32>,
    #[serde(default)]
    pub grid: i32,
    /// The food the server last sent, and the other entities it said died (issue 12a).
    #[serde(default)]
    pub food: Option<i32>,
    #[serde(default)]
    pub kills: u64,
    /// The blocks at the moment's `watch` cells, by name, at the end (issue 14a).
    #[serde(default)]
    pub blocks: Vec<String>,
    /// The placed mobs' outcomes, in the moment's order (issue 17).
    #[serde(default)]
    pub mobs: Vec<MobOutcome>,
    /// The thrown eye of ender's outcome, for a row that throws one (issue 19, y01 and y02).
    #[serde(default)]
    pub eye: Option<EyeOutcome>,
}

/// What became of an eye of ender the bot threw (issue 19, docs/problems/eyes-model.md), as the
/// bot's world saw it. Whether it dropped or shattered is the jar's roll, never the model's to
/// match, so it is reported in the row's eye trace, not in the class.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EyeOutcome {
    /// An eye entity was added near the bot after the throw.
    pub seen: bool,
    /// Its fitted line on the plane points within the row's tolerance of the bearing from the
    /// throw to the row's stronghold.
    pub on_line: bool,
    /// The server removed it within the run (its life ended).
    pub ended: bool,
    /// For a row that fetches it: no dropped eye is left on the ground at the end (it shattered,
    /// or it dropped and was picked up). `None` for a row that only throws.
    #[serde(default)]
    pub fetched: Option<bool>,
}

/// One run of a moment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MomentResult {
    pub moment: String,
    pub source: Source,
    pub commit: String,
    /// `None` when the run never got to the script (it is then a failed row, not a match).
    pub outcome: Option<Outcome>,
    /// Corrections counted, and why the run ended, for reading a mismatch.
    pub corrections: u64,
    pub ending: String,
    pub final_pos: Option<[f64; 3]>,
}

/// One row of the table.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub moment: String,
    pub real: Option<Outcome>,
    pub sim: Option<Outcome>,
    /// Field names that differ; empty when the classes match.
    pub differ: Vec<&'static str>,
}

pub fn compare(real: &[MomentResult], sim: &[MomentResult]) -> Vec<Row> {
    let by = |rs: &[MomentResult]| -> BTreeMap<String, Option<Outcome>> {
        rs.iter()
            .map(|r| (r.moment.clone(), r.outcome.clone()))
            .collect()
    };
    let (r, s) = (by(real), by(sim));
    let ids: std::collections::BTreeSet<&String> = r.keys().chain(s.keys()).collect();
    ids.into_iter()
        .map(|id| {
            let (real, sim) = (r.get(id).cloned().flatten(), s.get(id).cloned().flatten());
            let differ = match (&real, &sim) {
                (Some(a), Some(b)) => [
                    ("cell", a.cell != b.cell),
                    ("on_ground", a.on_ground != b.on_ground),
                    ("in_water", a.in_water != b.in_water),
                    ("corrected", a.corrected != b.corrected),
                    ("health", a.health != b.health),
                    ("inventory", a.inventory != b.inventory),
                    ("grid", a.grid != b.grid),
                    ("food", a.food != b.food),
                    ("kills", a.kills != b.kills),
                    ("blocks", a.blocks != b.blocks),
                    ("mobs", a.mobs != b.mobs),
                    ("eye", a.eye != b.eye),
                ]
                .into_iter()
                .filter_map(|(n, d)| d.then_some(n))
                .collect(),
                _ => vec!["missing"],
            };
            Row {
                moment: id.clone(),
                real,
                sim,
                differ,
            }
        })
        .collect()
}

/// The table as markdown, with the totals and the stated tolerance's verdict.
pub fn render(rows: &[Row], tolerance: usize) -> (String, bool) {
    let show = |o: &Option<Outcome>| {
        o.as_ref().map_or("-".to_owned(), |o| {
            let held = if o.inventory.is_empty() && o.grid == 0 {
                String::new()
            } else {
                format!(" {:?} grid {}", o.inventory, o.grid)
            };
            let life = match (o.food, o.kills) {
                (Some(f), k) if f < 20 || k > 0 => format!(" food {f} kills {k}"),
                (None, k) if k > 0 => format!(" kills {k}"),
                _ => String::new(),
            };
            let mobs: String = o
                .mobs
                .iter()
                .map(|m| {
                    format!(
                        " {}{}{}{}{}{}",
                        m.kind,
                        if m.seen { "" } else { " unseen" },
                        if m.moved { " moved" } else { " still" },
                        if m.creepy { " creepy" } else { "" },
                        if m.stared { " stared" } else { "" },
                        m.cell.map_or_else(String::new, |c| format!(" at {c:?}"))
                    )
                })
                .collect();
            let eye = o.eye.as_ref().map_or_else(String::new, |e| {
                format!(
                    " eye{}{}{}{}",
                    if e.seen { "" } else { " unseen" },
                    if e.on_line { " on line" } else { " off line" },
                    if e.ended { " ended" } else { " flying" },
                    match e.fetched {
                        Some(true) => " fetched",
                        Some(false) => " left",
                        None => "",
                    }
                )
            });
            let held = held + &life + &mobs + &eye;
            format!(
                "{:?} {} {} {} {:?}{held}",
                o.cell,
                if o.on_ground { "ground" } else { "air" },
                if o.in_water { "water" } else { "dry" },
                if o.corrected { "corrected" } else { "clean" },
                o.health
            )
        })
    };
    let matched = rows.iter().filter(|r| r.differ.is_empty()).count();
    let pass = matched >= tolerance && rows.len() >= tolerance;
    let mut lines = vec![
        format!(
            "**{matched} of {} moments match in every field** (tolerance: at least {tolerance}): {}",
            rows.len(),
            if pass { "PASS" } else { "FAIL" }
        ),
        String::new(),
        "| moment | real | sim | differs |".to_owned(),
        "|---|---|---|---|".to_owned(),
    ];
    for r in rows {
        lines.push(format!(
            "| {} | {} | {} | {} |",
            r.moment,
            show(&r.real),
            show(&r.sim),
            if r.differ.is_empty() {
                "-".to_owned()
            } else {
                r.differ.join(", ")
            }
        ));
    }
    let mut per_field: BTreeMap<&str, usize> = BTreeMap::new();
    for d in rows.iter().flat_map(|r| &r.differ) {
        *per_field.entry(d).or_default() += 1;
    }
    lines.push(String::new());
    lines.push(format!("disagreements by field: {per_field:?}"));
    (lines.join("\n") + "\n", pass)
}
