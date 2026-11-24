//! The ledger as text (docs/problems/observability-gaps.md). The `.rsl` keeps every record and
//! every fact; the text is what a person reads at two in the morning, so it says each thing once:
//! the planner's records in the order the planner made them, a run of the same records as one
//! line with its count and its last tick, and a second's changes to one item as one line. Nothing
//! is cut short: a line is as long as its note.

use std::collections::BTreeSet;

use ::observe::{Fact, Stamped};
use plan::TaskRecord;

/// The longest block of planner lines looked for as a repeat: a start, its end and two notes.
pub const MAX_PERIOD: usize = 4;
/// Times a block of two or more lines must come in a row to be told once (one line alone is
/// told once from its second copy).
pub const MIN_REPEATS: usize = 3;

/// One planner record as a line: its tick, its text, the text with the task's number left out
/// (what two records must share to be the same), and the number.
struct Line {
    at: u64,
    text: String,
    key: String,
    id: Option<u64>,
}

fn planner_raw(tasks: &[TaskRecord]) -> Vec<Line> {
    // A note that only repeats a task's start or end at the same tick is not shown twice.
    let told: BTreeSet<(u64, &str)> = tasks
        .iter()
        .filter_map(|t| match t {
            TaskRecord::Started { at, what, .. } => what.as_deref().map(|w| (*at, w)),
            TaskRecord::Ended { at, outcome, .. } => outcome.as_deref().map(|o| (*at, o)),
            TaskRecord::Note { .. } => None,
        })
        .collect();
    let mut out = Vec::new();
    for t in tasks {
        match t {
            TaskRecord::Started {
                id,
                at,
                task,
                what,
                why,
            } => {
                let rest = format!(
                    "{task}: {} ({})",
                    what.as_deref().unwrap_or("-"),
                    why.as_deref().unwrap_or("no reason")
                );
                out.push(Line {
                    at: *at,
                    text: format!("start #{id} {rest}"),
                    key: format!("start {rest}"),
                    id: Some(*id),
                });
            }
            TaskRecord::Ended {
                id,
                at,
                task,
                ticks,
                outcome,
                why,
            } => {
                let rest = format!(
                    "{task} after {ticks} ticks: {} ({})",
                    outcome.as_deref().unwrap_or("-"),
                    why.as_deref().unwrap_or("-")
                );
                out.push(Line {
                    at: *at,
                    text: format!("end #{id} {rest}"),
                    key: format!("end {rest}"),
                    id: Some(*id),
                });
            }
            TaskRecord::Note { at, what, why } => {
                if told.contains(&(*at, what.as_str())) {
                    continue;
                }
                // A note read back from a recording is one string, with its reason inside.
                let text = if why.is_empty() {
                    format!("note {what}")
                } else {
                    format!("note {what} ({why})")
                };
                out.push(Line {
                    at: *at,
                    key: text.clone(),
                    text,
                    id: None,
                });
            }
        }
    }
    out
}

/// The planner's records as lines, in the order it made them (a task that began and ended in
/// one second reads start then end, never the other way), each run of the same block told once:
/// `note cast paused: no pickaxe (…)  [x23982, to tick 28980]`.
pub fn planner_lines(tasks: &[TaskRecord]) -> Vec<(u64, String)> {
    let raw = planner_raw(tasks);
    let same = |a: usize, b: usize| {
        raw.get(a)
            .zip(raw.get(b))
            .is_some_and(|(x, y)| x.key == y.key)
    };
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(first) = raw.get(i) {
        // The block length and count that tell the most lines once.
        let mut best = (1usize, 1usize);
        for p in 1..=MAX_PERIOD {
            let mut k = 1;
            while i + (k + 1) * p <= raw.len() && (0..p).all(|j| same(i + j, i + k * p + j)) {
                k += 1;
            }
            let min = if p == 1 { 2 } else { MIN_REPEATS };
            if k >= min && k * p > best.0 * best.1 {
                best = (p, k);
            }
        }
        let (p, k) = best;
        if k == 1 {
            out.push((first.at, first.text.clone()));
            i += 1;
            continue;
        }
        for j in 0..p {
            let (Some(a), Some(z)) = (raw.get(i + j), raw.get(i + (k - 1) * p + j)) else {
                continue;
            };
            let last = z.id.map_or_else(String::new, |id| format!(" (#{id})"));
            out.push((a.at, format!("{}  [x{k}, to tick {}{last}]", a.text, z.at)));
        }
        i += p * k;
    }
    out
}

/// What makes two facts of one second the same line.
#[derive(PartialEq)]
enum Key {
    Item(String),
    Health,
    Food,
    Cursor,
    Other(String),
}

/// The facts of one second that share a key: the first, the last and how many.
struct Group<'a> {
    key: Key,
    first: &'a Fact,
    last: &'a Fact,
    n: usize,
    /// What the cursor held, in order (a cursor group only).
    held: Vec<String>,
}

fn key_of(f: &Fact) -> Key {
    match f {
        Fact::Item { item, .. } => Key::Item(item.clone()),
        Fact::Health { .. } => Key::Health,
        Fact::Food { .. } => Key::Food,
        Fact::Cursor { .. } => Key::Cursor,
        Fact::Hit { .. }
        | Fact::Alight { .. }
        | Fact::ToolWorn { .. }
        | Fact::ToolBroke { .. }
        | Fact::ToolGone { .. }
        | Fact::HealthDrop { .. }
        | Fact::Dimension { .. }
        | Fact::Stage { .. }
        | Fact::Death
        | Fact::Won
        | Fact::Effect { .. }
        | Fact::Sound { .. }
        | Fact::LevelEvent { .. }
        | Fact::Chat { .. }
        | Fact::Appear { .. }
        | Fact::Gone { .. }
        | Fact::Shot { .. }
        | Fact::Landed { .. }
        | Fact::Sign { .. }
        | Fact::Night { .. }
        | Fact::Day { .. } => Key::Other(f.line()),
    }
}

fn group_line(g: &Group) -> String {
    if g.n == 1 {
        return g.first.line();
    }
    let n = g.n;
    match (&g.key, g.first, g.last) {
        (Key::Item(item), Fact::Item { from, .. }, Fact::Item { to, .. }) => {
            let net = if from == to { ", none in the end" } else { "" };
            format!("item {item} {from} -> {to} ({n} changes{net})")
        }
        (Key::Health, Fact::Health { from, .. }, Fact::Health { to, .. }) => match from {
            Some(f) => format!("health {f:.1} -> {to:.1} ({n} changes)"),
            None => format!("health {to:.1} ({n} changes)"),
        },
        (Key::Food, Fact::Food { from, .. }, Fact::Food { to, .. }) => match from {
            Some(f) => format!("food {f} -> {to} ({n} changes)"),
            None => format!("food {to} ({n} changes)"),
        },
        (Key::Cursor, _, last) => {
            format!("{} ({n} changes: {})", last.line(), g.held.join(", "))
        }
        (Key::Item(_) | Key::Health | Key::Food | Key::Other(_), _, _) => {
            format!("{} x{n}", g.first.line())
        }
    }
}

/// The facts as lines: within one second (the ledger's stamp is the server's second) every
/// change to one item is one line with its first and last count and the number of changes, the
/// same for health, food and the cursor, and a fact that repeats word for word is told once
/// with its count.
pub fn fact_lines(facts: &[Stamped]) -> Vec<(u64, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(first) = facts.get(i) {
        let at = first.at;
        let mut groups: Vec<Group> = Vec::new();
        while let Some(f) = facts.get(i).filter(|f| f.at == at) {
            let key = key_of(&f.fact);
            let held = if let Fact::Cursor { item, count } = &f.fact {
                item.as_ref()
                    .map_or_else(|| "empty".to_owned(), |i| format!("{i} {count}"))
            } else {
                String::new()
            };
            match groups.iter_mut().find(|g| g.key == key) {
                Some(g) => {
                    g.last = &f.fact;
                    g.n += 1;
                    g.held.push(held);
                }
                None => groups.push(Group {
                    key,
                    first: &f.fact,
                    last: &f.fact,
                    n: 1,
                    held: vec![held],
                }),
            }
            i += 1;
        }
        out.extend(groups.iter().map(|g| (at, group_line(g))));
    }
    out
}

/// Planner lines and fact lines as one list by tick: at one tick the planner's lines come
/// first, in their own order, then the facts in theirs (the two are kept apart in the file, so
/// their order within one second is not known).
pub fn merged(tasks: &[TaskRecord], facts: &[Stamped]) -> Vec<(u64, String)> {
    let mut lines: Vec<(u64, u8, String)> = planner_lines(tasks)
        .into_iter()
        .map(|(at, l)| (at, 0, l))
        .chain(fact_lines(facts).into_iter().map(|(at, l)| (at, 1, l)))
        .collect();
    // Stable: lines of one tick and one side keep their order.
    lines.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    lines.into_iter().map(|(at, _, l)| (at, l)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(at: u64, what: &str, why: &str) -> TaskRecord {
        TaskRecord::Note {
            at,
            what: what.to_owned(),
            why: why.to_owned(),
        }
    }

    fn item(at: u64, item: &str, from: i32, to: i32) -> Stamped {
        Stamped {
            at,
            fact: Fact::Item {
                item: item.to_owned(),
                from,
                to,
            },
        }
    }

    /// Wall 4: one bot's ledger held 23,982 copies of "note cast paused: no
    /// pickaxe". A run is one line with its count and its last tick; the notes round it stand.
    #[test]
    fn a_run_of_the_same_note_is_one_line_with_its_count() {
        let mut tasks = vec![note(4980, "craft stone_pickaxe failed: Unopened", "settle")];
        tasks.extend((0..23_982u64).map(|i| note(5000 + i, "cast paused: no pickaxe", "wait")));
        tasks.push(note(29_000, "cast resumed", "a pickaxe held"));
        let lines = planner_lines(&tasks);
        assert_eq!(
            lines,
            vec![
                (
                    4980,
                    "note craft stone_pickaxe failed: Unopened (settle)".to_owned()
                ),
                (
                    5000,
                    "note cast paused: no pickaxe (wait)  [x23982, to tick 28981]".to_owned()
                ),
                (29_000, "note cast resumed (a pickaxe held)".to_owned()),
            ]
        );
    }

    /// Wall 7a (lane 3): a task that began and ended in one second was printed end first, and
    /// 600 such pairs filled the ledger. The planner's order is kept, and a run of pairs that
    /// differ only in the task's number is told once.
    #[test]
    fn tasks_keep_the_planners_order_and_a_run_of_pairs_is_told_once() {
        let mut tasks = Vec::new();
        for (i, at) in [22_220u64, 22_240, 22_260].into_iter().enumerate() {
            let id = 57 + i as u64;
            let what = "stairs down toward the fortress at (61, 32, 95)";
            tasks.push(note(at, what, "nether bricks seen, no walk to them"));
            tasks.push(TaskRecord::Started {
                id,
                at,
                task: "stairs".to_owned(),
                what: Some(what.to_owned()),
                why: Some("nether bricks seen, no walk to them".to_owned()),
            });
            tasks.push(TaskRecord::Ended {
                id,
                at,
                task: "stairs".to_owned(),
                ticks: 0,
                outcome: Some("stairs failed: StepFailed after 0 stairs".to_owned()),
                why: Some("turn the heading".to_owned()),
            });
        }
        tasks.push(TaskRecord::Started {
            id: 60,
            at: 22_280,
            task: "explore".to_owned(),
            what: Some("explore toward (67, 33, -18)".to_owned()),
            why: None,
        });
        let lines = planner_lines(&tasks);
        assert_eq!(
            lines,
            vec![
                (
                    22_220,
                    "start #57 stairs: stairs down toward the fortress at (61, 32, 95) (nether bricks seen, no walk to them)  [x3, to tick 22260 (#59)]".to_owned()
                ),
                (
                    22_220,
                    "end #57 stairs after 0 ticks: stairs failed: StepFailed after 0 stairs (turn the heading)  [x3, to tick 22260 (#59)]".to_owned()
                ),
                (
                    22_280,
                    "start #60 explore: explore toward (67, 33, -18) (no reason)".to_owned()
                ),
            ]
        );
        // Two pairs are not a run: each is told, start before end.
        let two = planner_lines(&tasks[..6]);
        assert_eq!(two.len(), 4);
        assert!(two[0].1.starts_with("start #57"), "{two:?}");
        assert!(two[1].1.starts_with("end #57"), "{two:?}");
        assert!(two[2].1.starts_with("start #58"), "{two:?}");
    }

    /// Wall 3: "item cobbled_deepslate 164 -> 170, 170 -> 173, 173 -> 177" in one second is one
    /// line; a stack that left and came back in the second (a hotbar swap) says so; the count
    /// of changes is kept.
    #[test]
    fn one_seconds_changes_to_an_item_are_one_line() {
        let facts = vec![
            item(14_240, "cobbled_deepslate", 164, 170),
            item(14_240, "stone_pickaxe", 1, 0),
            item(14_240, "cobbled_deepslate", 170, 173),
            item(14_240, "stone_pickaxe", 0, 1),
            item(14_240, "cobbled_deepslate", 173, 177),
            item(14_280, "cobbled_deepslate", 177, 178),
        ];
        assert_eq!(
            fact_lines(&facts),
            vec![
                (
                    14_240,
                    "item cobbled_deepslate 164 -> 177 (3 changes)".to_owned()
                ),
                (
                    14_240,
                    "item stone_pickaxe 1 -> 1 (2 changes, none in the end)".to_owned()
                ),
                (14_280, "item cobbled_deepslate 177 -> 178".to_owned()),
            ]
        );
    }

    /// The other facts of a second: health as one span, a sound heard four times once with its
    /// count, the cursor's last state with what it held.
    #[test]
    fn one_seconds_health_sounds_and_cursor_are_told_once() {
        let at = 22_720;
        let stamp = |fact| Stamped { at, fact };
        let burn = || Fact::Sound {
            name: "entity.generic.burn".to_owned(),
            d: Some(0.2),
        };
        let facts = vec![
            stamp(Fact::Health {
                from: Some(17.8),
                to: 15.5,
            }),
            stamp(burn()),
            stamp(Fact::Health {
                from: Some(15.5),
                to: 13.3,
            }),
            stamp(burn()),
            stamp(Fact::Cursor {
                item: Some("stick".to_owned()),
                count: 2,
            }),
            stamp(Fact::Cursor {
                item: None,
                count: 0,
            }),
        ];
        let lines: Vec<String> = fact_lines(&facts).into_iter().map(|l| l.1).collect();
        assert_eq!(
            lines,
            vec![
                "health 17.8 -> 13.3 (2 changes)",
                "heard entity.generic.burn at 0 x2",
                "cursor empty (2 changes: stick 2, empty)",
            ]
        );
    }

    /// Wall 6: an escape's trail is a long note, and it is shown whole.
    #[test]
    fn a_long_note_is_never_cut() {
        let trail = (0..60)
            .map(|i| format!("{i} sealed the door at (30, 32, -{i}) with netherrack"))
            .collect::<Vec<_>>()
            .join(" / ");
        assert!(trail.len() > 2000);
        let tasks = vec![note(
            22_700,
            "out of the lava",
            &format!("back to the goal; trail: {trail}"),
        )];
        let lines = merged(&tasks, &[]);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].1.ends_with(&format!("{trail})")), "{lines:?}");
    }
}
