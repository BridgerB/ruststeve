//! A run's story for its case file (issue 16e, phase C): built from the ledger and the senses,
//! never cut. What a reader needs in ten seconds (the stages, the tool and item events with the
//! task running at each, when the bot began to starve, went without a pickaxe, burned or ran low,
//! and the tasks that took the time), where the bot stood at the end in plain terms, every note
//! counted by kind (so a fix's firings are read here, with no grep), each escape's trail whole,
//! then the last two minutes as the ledger tells them.

use serde::{Deserialize, Serialize};

use crate::Ledger;
use ::observe::{Fact, Senses, Stamped};

/// Ticks of the end shown in full (two minutes).
pub const LAST_TICKS: u64 = 2400;
/// Lines of the end shown at most.
pub const LAST_LINES: usize = 80;
/// Tasks listed by the time they took.
pub const LONGEST: usize = 5;
/// Health under this is low (the defend row's danger line is 6, `config/goals.toml`).
pub const LOW_HEALTH: f32 = 6.0;
/// Words of a note that make its kind, at most ("drop Cobblestone from slot").
pub const KIND_WORDS: usize = 5;
/// What a note says when it carries an escape's trail (`plan`: "…; trail: a / b / c").
pub const TRAIL: &str = "trail:";

/// One kind of planner note in a run: how many times it was made, and its first and last tick.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteKind {
    pub kind: String,
    pub n: u64,
    pub first: u64,
    pub last: u64,
}

/// A note's kind from what it says: its leading words up to the first with a number or a
/// bracket, at most `KIND_WORDS`, an item's name (a word with an underscore) as `*`, so notes
/// that differ only in a place, a count or an item are one kind ("seal lava at", "magma under",
/// "craft * failed: Unopened"). A failed cast is its failure and its op, as the diagnosis has it.
pub fn note_kind(what: &str) -> String {
    if let Some(f) = crate::diagnose::cast_failure(what) {
        return format!("cast failed: {} at {}", f.failure, f.op);
    }
    let plain = |w: &&str| {
        !w.chars()
            .any(|c| c.is_ascii_digit() || c == '(' || c == '[')
    };
    let words: Vec<&str> = what
        .split_whitespace()
        .take_while(plain)
        .take(KIND_WORDS)
        .map(|w| if w.contains('_') { "*" } else { w })
        .collect();
    if words.is_empty() {
        return what.split_whitespace().next().unwrap_or("").to_owned();
    }
    words.join(" ").trim_end_matches([':', ',', ';']).to_owned()
}

/// Every note of a ledger by kind, the most made first.
pub fn note_kinds(l: &Ledger) -> Vec<NoteKind> {
    let mut kinds: std::collections::BTreeMap<String, NoteKind> = Default::default();
    for t in &l.tasks {
        if let plan::TaskRecord::Note { at, what, .. } = t {
            let kind = note_kind(what);
            let k = kinds.entry(kind.clone()).or_insert(NoteKind {
                kind,
                n: 0,
                first: *at,
                last: *at,
            });
            k.n += 1;
            k.first = k.first.min(*at);
            k.last = k.last.max(*at);
        }
    }
    let mut out: Vec<NoteKind> = kinds.into_values().collect();
    out.sort_by(|a, b| b.n.cmp(&a.n).then_with(|| a.kind.cmp(&b.kind)));
    out
}

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
    /// Task kinds by count and total ticks, with those that ended within the second they began:
    /// `stairs 680 x, 19200 ticks (644 ended within the second they began)`.
    pub kinds: Vec<String>,
    /// The last two minutes of the ledger, one line each.
    pub last: Vec<String>,
    /// What the server held that the bot did not, at the end (`truth::blind`).
    #[serde(default)]
    pub blind: Vec<String>,
    /// Where the bot stood at the end, in plain terms, for the case's head: the dimension, the
    /// position and the feet's cell, what the feet stood on, lava or water against the body, what
    /// it held, the task and its reason, the time since the last stage.
    #[serde(default)]
    pub standing: Vec<String>,
    /// Every planner note of the run by kind (`note_kinds`): a new code path's firings.
    #[serde(default)]
    pub note_kinds: Vec<NoteKind>,
    /// Every note that carries an escape's trail, whole: `tick what (why)`.
    #[serde(default)]
    pub trails: Vec<String>,
}

/// The tick of a ledger's last record or fact.
fn end_tick(l: &Ledger) -> u64 {
    l.facts
        .last()
        .map(|f| f.at)
        .into_iter()
        .chain(l.tasks.iter().map(at_of))
        .max()
        .unwrap_or(0)
}

fn at_of(t: &plan::TaskRecord) -> u64 {
    match t {
        plan::TaskRecord::Started { at, .. }
        | plan::TaskRecord::Ended { at, .. }
        | plan::TaskRecord::Note { at, .. } => *at,
    }
}

/// Where the bot stood at the end: the last senses row and the ledger's last stage.
fn standing(l: &Ledger, rows: &[Senses]) -> Vec<String> {
    let Some(r) = rows.last() else {
        return Vec::new();
    };
    let end = end_tick(l).max(r.at);
    let pos = r.pos.map_or_else(
        || "-".to_owned(),
        |p| format!("{:.2},{:.2},{:.2}", p[0], p[1], p[2]),
    );
    let mut out = vec![
        format!(
            "in {} at {pos}: {}",
            r.dimension.as_deref().unwrap_or("-"),
            r.standing()
        ),
        format!(
            "health {}, food {}, held {}{}{}, {} free slots",
            r.health
                .map_or_else(|| "-".to_owned(), |h| format!("{h:.1}")),
            r.food.map_or_else(|| "-".to_owned(), |f| f.to_string()),
            r.held.as_deref().unwrap_or("nothing"),
            r.held_uses
                .map_or_else(String::new, |u| format!(" ({u} uses)")),
            r.cursor
                .as_ref()
                .map_or_else(String::new, |(i, n)| format!(", {i} {n} on the cursor")),
            r.free_slots
        ),
        format!("task {} ({}) at tick {}", r.task, r.why, r.at),
    ];
    let stage = l.facts.iter().rev().find_map(|f| {
        if let Fact::Stage { reached } = &f.fact {
            Some((f.at, reached))
        } else {
            None
        }
    });
    out.push(match stage {
        Some((at, reached)) => format!(
            "last stage {reached} at tick {at}, {} ticks before the end (tick {end})",
            end.saturating_sub(at)
        ),
        None => format!("no stage reached in this ledger (it ends at tick {end})"),
    });
    out
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
            plan::TaskRecord::Started { .. }
            | plan::TaskRecord::Ended { .. }
            | plan::TaskRecord::Note { .. } => {}
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
    let all_rows = rows;
    // Rows before the inventory first arrives (every slot free) say nothing about tools.
    let first = rows
        .iter()
        .position(|r| r.free_slots < 36)
        .unwrap_or(rows.len());
    let rows = rows.get(first..).unwrap_or_default();
    for f in &l.facts {
        match &f.fact {
            Fact::Stage { reached } => s.stages.push(format!("{} {reached}", f.at)),
            Fact::ToolWorn { .. } | Fact::ToolBroke { .. } | Fact::ToolGone { .. } => {
                let during =
                    running_at(l, f.at).map_or_else(String::new, |t| format!(" (during {t})"));
                s.tools.push(format!("{} {}{during}", f.at, f.fact.line()));
            }
            _ => {}
        }
    }
    // A pickaxe on the cursor (mid-click) is still a pickaxe held.
    let pickaxe = |r: &Senses| {
        r.tools
            .iter()
            .any(|(t, u)| t.ends_with("_pickaxe") && *u > 0)
            || r.cursor
                .as_ref()
                .is_some_and(|(i, _)| i.ends_with("_pickaxe"))
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
    // kind -> (tasks, ticks in all, tasks that ended within the second they began)
    let mut kinds: std::collections::BTreeMap<String, (u64, u64, u64)> = Default::default();
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
                    plan::TaskRecord::Started { .. }
                    | plan::TaskRecord::Ended { .. }
                    | plan::TaskRecord::Note { .. } => None,
                })
                .unwrap_or_default();
            ended.push((*ticks, *id, task.clone(), what));
            let k = kinds.entry(task.clone()).or_default();
            k.0 += 1;
            k.1 += ticks;
            k.2 += u64::from(*ticks == 0);
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
        .map(|(k, (n, t, zero))| {
            // A task that ends as it begins did nothing: 644 of lane 3's 680 stairs (race
            // 45204423376) were refused so, which "680 x, 19200 ticks" did not say.
            let instant = if *zero > 0 {
                format!(" ({zero} ended within the second they began)")
            } else {
                String::new()
            };
            format!("{k} {n} x, {t} ticks{instant}")
        })
        .collect();
    // The last two minutes as the ledger tells them: the records and facts of those ticks alone,
    // so a run of the same note that began an hour before still shows (told from the window's
    // first tick, with its count inside the window).
    let from = end_tick(l).saturating_sub(LAST_TICKS);
    let window = Ledger {
        format_version: l.format_version,
        run: l.run.clone(),
        bot: l.bot.clone(),
        commit: l.commit.clone(),
        tasks: l
            .tasks
            .iter()
            .filter(|t| at_of(t) >= from)
            .cloned()
            .collect(),
        dropped: l.dropped,
        facts: l
            .facts
            .iter()
            .filter(|f: &&Stamped| f.at >= from)
            .cloned()
            .collect(),
    };
    let tail: Vec<String> = window
        .lines()
        .into_iter()
        .map(|(at, line)| format!("{at:>8} {line}"))
        .collect();
    s.last = tail
        .iter()
        .skip(tail.len().saturating_sub(LAST_LINES))
        .cloned()
        .collect();
    s.standing = standing(l, all_rows);
    s.note_kinds = note_kinds(l);
    s.trails = l
        .tasks
        .iter()
        .filter_map(|t| match t {
            plan::TaskRecord::Note { at, what, why }
                if what.contains(TRAIL) || why.contains(TRAIL) =>
            {
                Some(format!("{at} {what} ({why})"))
            }
            plan::TaskRecord::Note { .. }
            | plan::TaskRecord::Started { .. }
            | plan::TaskRecord::Ended { .. } => None,
        })
        .collect();
    s
}

impl Story {
    /// The story's lines for the case file, under headings (`standing` is the case's head,
    /// printed there).
    pub fn render(&self) -> Vec<String> {
        let mut out = Vec::new();
        let kinds: Vec<String> = self
            .note_kinds
            .iter()
            .map(|k| format!("{:>6} x {} (ticks {} to {})", k.n, k.kind, k.first, k.last))
            .collect();
        for (head, lines) in [
            ("stages", &self.stages),
            ("tools", &self.tools),
            ("onsets", &self.onsets),
            ("longest tasks", &self.longest),
            ("tasks by time", &self.kinds),
            (
                "notes by kind (every note of the ledger; a fix's firings are counted here)",
                &kinds,
            ),
            ("escape trails (whole)", &self.trails),
            ("blind (server truth at the end)", &self.blind),
            (
                "the last two minutes (as the ledger tells them: a run of the same records is one line)",
                &self.last,
            ),
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
            cell: None,
            feet_block: None,
            under: None,
            lava: ::observe::Touch::No,
            water: ::observe::Touch::No,
            cursor: None,
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

    fn note(at: u64, what: &str, why: &str) -> plan::TaskRecord {
        plan::TaskRecord::Note {
            at,
            what: what.to_owned(),
            why: why.to_owned(),
        }
    }

    fn ledger(tasks: Vec<plan::TaskRecord>, facts: Vec<Stamped>) -> Ledger {
        Ledger {
            format_version: 0,
            run: "r".to_owned(),
            bot: "b".to_owned(),
            commit: "c".to_owned(),
            tasks,
            dropped: 0,
            facts,
        }
    }

    /// Wall 5: notes that differ in a place, a count or an item are one kind, so a fix's
    /// firings are one row of the case's table.
    #[test]
    fn a_notes_kind_is_its_leading_words() {
        for (what, kind) in [
            ("seal lava at (30, 32, -20) with netherrack", "seal lava at"),
            ("magma under (31, 29, -19)", "magma under"),
            ("nether kit held", "nether kit held"),
            ("bridge", "bridge"),
            ("travel tunnel toward (61, 31, 95)", "travel tunnel toward"),
            ("cast paused: no pickaxe", "cast paused: no pickaxe"),
            ("relocate (1 of 6)", "relocate"),
            (
                "craft stone_pickaxe failed: Unopened",
                "craft * failed: Unopened",
            ),
            (
                "craft stone_pickaxe at the table (121, -21, -111)",
                "craft * at the table",
            ),
            (
                "drop Cobblestone from slot 21",
                "drop Cobblestone from slot",
            ),
            (
                "stairs failed: StepFailed after 0 stairs",
                "stairs failed: StepFailed after",
            ),
            ("escape-lava:", "escape-lava"),
            ("(61, 31, 95) refused", "(61,"),
        ] {
            assert_eq!(note_kind(what), kind, "{what}");
        }
    }

    /// Walls 4, 5 and 6 in one case: 3,000 copies of one note fill the last two minutes and
    /// began long before them; the story counts every note by kind, tells the run as one line
    /// of the tail, and keeps the escape's trail whole.
    #[test]
    fn the_story_counts_notes_by_kind_and_tells_a_run_once() {
        let trail = "sealed (30, 32, -20) with netherrack / stepped to (31, 31, -19) / ".repeat(8);
        let mut tasks = vec![
            note(100, "seal lava at (30, 32, -20)", "lava at the door"),
            note(160, "seal lava at (31, 32, -20)", "lava at the door"),
            note(
                200,
                "out of the lava",
                &format!("back to the goal; trail: {trail}"),
            ),
        ];
        tasks.extend((0..3_000u64).map(|i| note(1_000 + i * 4, "cast paused: no pickaxe", "wait")));
        let facts = vec![
            Stamped {
                at: 20,
                fact: Fact::Stage {
                    reached: "nether".to_owned(),
                },
            },
            Stamped {
                at: 12_996,
                fact: Fact::Item {
                    item: "netherrack".to_owned(),
                    from: 3,
                    to: 4,
                },
            },
        ];
        let l = ledger(tasks, facts);
        let mut last = row(12_980, 16, Some(40));
        last.pos = Some([30.499, 31.0, -18.3]);
        last.cell = Some([30, 31, -19]);
        last.feet_block = Some("air".to_owned());
        last.under = Some("soul_sand".to_owned());
        last.lava = ::observe::Touch::Beside;
        last.dimension = Some("minecraft:the_nether".to_owned());
        last.task = "idle".to_owned();
        last.why = "12 tasks failed in a row with none done".to_owned();
        let s = story(&l, &[row(400, 16, Some(40)), last]);
        assert_eq!(
            s.note_kinds,
            vec![
                NoteKind {
                    kind: "cast paused: no pickaxe".to_owned(),
                    n: 3_000,
                    first: 1_000,
                    last: 12_996,
                },
                NoteKind {
                    kind: "seal lava at".to_owned(),
                    n: 2,
                    first: 100,
                    last: 160,
                },
                NoteKind {
                    kind: "out of the lava".to_owned(),
                    n: 1,
                    first: 200,
                    last: 200,
                },
            ]
        );
        // The window is ticks 10,596 to 12,996: 601 of the 3,000 notes, told once.
        assert_eq!(
            s.last,
            vec![
                "   10596 note cast paused: no pickaxe (wait)  [x601, to tick 12996]",
                "   12996 item netherrack 3 -> 4",
            ]
        );
        assert_eq!(s.trails.len(), 1);
        assert!(
            s.trails[0].ends_with(&format!("{trail})")),
            "{:?}",
            s.trails
        );
        assert_eq!(
            s.standing,
            vec![
                "in minecraft:the_nether at 30.50,31.00,-18.30: cell 30,31,-19 on soul_sand, lava beside",
                "health 20.0, food 16, held nothing, 10 free slots",
                "task idle (12 tasks failed in a row with none done) at tick 12980",
                "last stage nether at tick 20, 12976 ticks before the end (tick 12996)",
            ]
        );
        let text = s.render().join("\n");
        assert!(
            text.contains("  3000 x cast paused: no pickaxe (ticks 1000 to 12996)"),
            "{text}"
        );
        assert!(text.contains("escape trails (whole):"), "{text}");
    }
}
