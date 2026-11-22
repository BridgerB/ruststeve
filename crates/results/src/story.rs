//! A run's story for its case file (issue 16e, phase C): built from the ledger and the senses,
//! never cut. What a reader needs in ten seconds (the stages, the tool and item events with the
//! task running at each, when the bot began to starve, went without a pickaxe, burned or ran low,
//! and the tasks that took the time), then the last two minutes in full.

use serde::{Deserialize, Serialize};

use crate::Ledger;
use ::observe::{Fact, Senses};

/// Ticks of the end shown in full (two minutes).
pub const LAST_TICKS: u64 = 2400;
/// Lines of the end shown at most.
pub const LAST_LINES: usize = 80;
/// Tasks listed by the time they took.
pub const LONGEST: usize = 5;
/// Health under this is low (the defend row's danger line is 6, `config/goals.toml`).
pub const LOW_HEALTH: f32 = 6.0;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Story {
    /// `tick stage`.
    pub stages: Vec<String>,
    /// Tools worn and broken, and the task running then: `tick tool broke: stone_pickaxe
    /// (during #31 stairs: stairs up toward the log at …)`.
    pub tools: Vec<String>,
    /// When a state began and how long it lasted, from the senses: starving, no pickaxe, low
    /// health, burning.
    pub onsets: Vec<String>,
    /// The tasks that ran longest: `ticks #id task: what`.
    pub longest: Vec<String>,
    /// Task kinds by count and total ticks: `stairs 41 x, 18,300 ticks`.
    pub kinds: Vec<String>,
    /// The last two minutes of the ledger, one line each.
    pub last: Vec<String>,
    /// What the server held that the bot did not, at the end (`truth::blind`).
    #[serde(default)]
    pub blind: Vec<String>,
}

/// The task open at `at`, as `#id task: what`.
fn running_at(l: &Ledger, at: u64) -> Option<String> {
    let mut open: Option<String> = None;
    for t in &l.tasks {
        match t {
            plan::TaskRecord::Started {
                id,
                at: s,
                task,
                what,
                ..
            } if *s <= at => {
                open = Some(format!("#{id} {task}: {}", what.as_deref().unwrap_or("-")));
            }
            plan::TaskRecord::Ended { at: e, .. } if *e < at => open = None,
            plan::TaskRecord::Started { .. } | plan::TaskRecord::Ended { .. } => {}
        }
    }
    open
}

/// Runs of rows where `on` holds: (first tick, ticks it lasted), the last run open to the end.
fn spans(rows: &[Senses], on: impl Fn(&Senses) -> bool) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut from: Option<u64> = None;
    for r in rows {
        match (on(r), from) {
            (true, None) => from = Some(r.at),
            (false, Some(f)) => {
                out.push((f, r.at.saturating_sub(f)));
                from = None;
            }
            (true, Some(_)) | (false, None) => {}
        }
    }
    if let (Some(f), Some(last)) = (from, rows.last()) {
        out.push((f, last.at.saturating_sub(f)));
    }
    out
}

pub fn story(l: &Ledger, rows: &[Senses]) -> Story {
    let mut s = Story::default();
    // Rows before the inventory first arrives (every slot free) say nothing about tools.
    let first = rows
        .iter()
        .position(|r| r.free_slots < 36)
        .unwrap_or(rows.len());
    let rows = rows.get(first..).unwrap_or_default();
    for f in &l.facts {
        match &f.fact {
            Fact::Stage { reached } => s.stages.push(format!("{} {reached}", f.at)),
            Fact::ToolWorn { .. } | Fact::ToolBroke { .. } => {
                let during =
                    running_at(l, f.at).map_or_else(String::new, |t| format!(" (during {t})"));
                s.tools.push(format!("{} {}{during}", f.at, f.fact.line()));
            }
            _ => {}
        }
    }
    let pickaxe = |r: &Senses| {
        r.tools
            .iter()
            .any(|(t, u)| t.ends_with("_pickaxe") && *u > 0)
    };
    for (name, list) in [
        ("starving (food 0)", spans(rows, |r| r.food == Some(0))),
        ("no pickaxe", spans(rows, |r| !pickaxe(r))),
        (
            "low health",
            spans(rows, |r| r.health.is_some_and(|h| h < LOW_HEALTH)),
        ),
        ("burning", spans(rows, |r| r.on_fire)),
    ] {
        let total: u64 = list.iter().map(|x| x.1).sum();
        if let Some((first, _)) = list.first() {
            s.onsets.push(format!(
                "{name}: from tick {first}, {} spans, {total} ticks in all",
                list.len()
            ));
        }
    }
    // Each task's length, from its end record.
    let mut ended: Vec<(u64, u64, String, String)> = Vec::new();
    let mut kinds: std::collections::BTreeMap<String, (u64, u64)> = Default::default();
    for t in &l.tasks {
        if let plan::TaskRecord::Ended {
            id, task, ticks, ..
        } = t
        {
            let what = l
                .tasks
                .iter()
                .find_map(|x| match x {
                    plan::TaskRecord::Started { id: i, what, .. } if i == id => what.clone(),
                    plan::TaskRecord::Started { .. } | plan::TaskRecord::Ended { .. } => None,
                })
                .unwrap_or_default();
            ended.push((*ticks, *id, task.clone(), what));
            let k = kinds.entry(task.clone()).or_default();
            k.0 += 1;
            k.1 += ticks;
        }
    }
    ended.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    s.longest = ended
        .iter()
        .take(LONGEST)
        .map(|(ticks, id, task, what)| format!("{ticks} ticks #{id} {task}: {what}"))
        .collect();
    let mut kinds: Vec<_> = kinds.into_iter().collect();
    kinds.sort_by_key(|k| std::cmp::Reverse(k.1.1));
    s.kinds = kinds
        .iter()
        .map(|(k, (n, t))| format!("{k} {n} x, {t} ticks"))
        .collect();
    let end = l
        .facts
        .last()
        .map(|f| f.at)
        .into_iter()
        .chain(l.tasks.iter().map(|t| match t {
            plan::TaskRecord::Started { at, .. } | plan::TaskRecord::Ended { at, .. } => *at,
        }))
        .max()
        .unwrap_or(0);
    let from = end.saturating_sub(LAST_TICKS);
    let all = l.render();
    let tail: Vec<String> = all
        .lines()
        .skip(1)
        .filter(|line| {
            line.split_whitespace()
                .next()
                .and_then(|t| t.parse::<u64>().ok())
                .is_some_and(|t| t >= from)
        })
        .map(str::to_owned)
        .collect();
    s.last = tail
        .iter()
        .skip(tail.len().saturating_sub(LAST_LINES))
        .cloned()
        .collect();
    s
}

impl Story {
    /// The story's lines for the case file, under headings.
    pub fn render(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (head, lines) in [
            ("stages", &self.stages),
            ("tools", &self.tools),
            ("onsets", &self.onsets),
            ("longest tasks", &self.longest),
            ("tasks by time", &self.kinds),
            ("blind (server truth at the end)", &self.blind),
            ("the last two minutes", &self.last),
        ] {
            if lines.is_empty() {
                continue;
            }
            out.push(format!("{head}:"));
            out.extend(lines.iter().map(|l| format!("  {l}")));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::observe::Stamped;

    fn row(at: u64, food: i32, pickaxe: Option<i32>) -> Senses {
        Senses {
            at,
            pos: None,
            dimension: None,
            health: Some(20.0),
            food: Some(food),
            saturation: None,
            air: None,
            on_fire: false,
            held: None,
            held_uses: None,
            tools: pickaxe
                .map(|u| vec![("stone_pickaxe".to_owned(), u)])
                .unwrap_or_default(),
            free_slots: 10,
            task: "stairs".to_owned(),
            why: String::new(),
            hostiles: Vec::new(),
            endermen: 0,
            sound: None,
            tick_us: None,
        }
    }

    /// Blaze wall 7 as the story tells it: the pickaxe broke during the stairs toward the log,
    /// the bot went without one from then, and starved; no "notes left out".
    #[test]
    fn the_story_names_the_break_its_task_and_the_onsets() {
        let l = Ledger {
            format_version: 0,
            run: "r".to_owned(),
            bot: "b".to_owned(),
            commit: "c".to_owned(),
            tasks: vec![
                plan::TaskRecord::Started {
                    id: 31,
                    at: 100,
                    task: "stairs".to_owned(),
                    what: Some("stairs up toward the log at (55, 72, -162)".to_owned()),
                    why: Some("a log over the feet with no walk to it".to_owned()),
                },
                plan::TaskRecord::Ended {
                    id: 31,
                    at: 900,
                    task: "stairs".to_owned(),
                    ticks: 800,
                    outcome: Some("cut 6 stairs".to_owned()),
                    why: None,
                },
            ],
            dropped: 0,
            facts: vec![Stamped {
                at: 500,
                fact: Fact::ToolBroke {
                    item: "stone_pickaxe".to_owned(),
                },
            }],
        };
        let rows = vec![
            row(400, 10, Some(3)),
            row(600, 10, None),
            row(800, 0, None),
            row(1000, 0, None),
        ];
        let s = story(&l, &rows);
        assert_eq!(s.tools.len(), 1);
        assert!(
            s.tools[0]
                .contains("tool broke: stone_pickaxe (during #31 stairs: stairs up toward the log"),
            "{:?}",
            s.tools
        );
        assert!(
            s.onsets
                .iter()
                .any(|o| o.starts_with("no pickaxe: from tick 600")),
            "{:?}",
            s.onsets
        );
        assert!(
            s.onsets
                .iter()
                .any(|o| o.starts_with("starving (food 0): from tick 800")),
            "{:?}",
            s.onsets
        );
        assert_eq!(
            s.longest,
            vec!["800 ticks #31 stairs: stairs up toward the log at (55, 72, -162)"]
        );
        assert!(!s.last.is_empty());
    }
}
