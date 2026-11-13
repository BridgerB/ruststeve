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
