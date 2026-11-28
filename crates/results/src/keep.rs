//! The keep rule's count per run (issue 77, brain/ruststeve/design/keep-rule.md): the drops,
//! the wants, and `dropped_then_wanted`, the wants of a kind the run had dropped earlier. The
//! issue's bar is that last count at 0. It is read from the ledger's notes: the drop lines
//! (`drop <Kind> from slot <n>`, `drop <Kind> from the cursor`) and the want lines
//! (`want <Kind>|<Kind> <n> for <task>`) `plan::keep` and `Plan::equip` write. The summary row
//! comes after issue 56 (lead-77).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use std::path::Path;

use crate::Ledger;

/// A run's keep counts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeepCounts {
    /// Drop lines.
    pub drops: u64,
    /// Want lines.
    pub wants: u64,
    /// Want lines naming a kind dropped at an earlier tick of the run (to be 0).
    pub dropped_then_wanted: u64,
    /// Those wants, as `<tick> <what>`, in order.
    pub cases: Vec<String>,
    /// Want lines by their source (`want_source`): craft, smelt, goal (the main planner), or a
    /// sub-planner's tag (base, eyes, end_portal, stronghold).
    #[serde(default)]
    pub wants_by_source: BTreeMap<String, u64>,
    /// Every want by source and kinds: the lines, and the count they asked for in all.
    #[serde(default)]
    pub want_list: BTreeMap<(String, String), WantTally>,
    /// Each dropped-then-wanted kind: the kind, its last drop's tick before the want, the want's.
    #[serde(default)]
    pub pairs: Vec<DropWant>,
    /// The stash and fetch paths (plan::stash): which ran, and how each gate came out.
    #[serde(default)]
    pub chest: ChestCounts,
}

/// One (source, kinds) of want lines: how many lines, and their counts summed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WantTally {
    pub lines: u64,
    pub n: i64,
}

/// A kind wanted after the run dropped it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropWant {
    pub kind: String,
    pub dropped_at: u64,
    pub wanted_at: u64,
}

/// The stash and fetch notes `plan::stash` writes, counted (their texts as stash.rs has them).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChestCounts {
    /// "stash done: ..." and "stash failed: ..." (`Plan::chest_step`'s end notes).
    pub stash_done: u64,
    pub stash_failed: u64,
    /// "fetch done: ..." and "fetch failed: ...".
    pub fetch_done: u64,
    pub fetch_failed: u64,
    /// The stash gate (`stash_before`): "stash before the <leg> at ...".
    pub gate_stashed: u64,
    /// "stash before the <leg>: nothing to put away".
    pub gate_nothing: u64,
    /// "stash before the <leg> skipped: danger".
    pub gate_danger: u64,
    /// "stash before the <leg> skipped: no home chest".
    pub gate_no_home: u64,
    /// "stash before the <leg> skipped" for "the keep table could not be read ...".
    pub gate_no_keep: u64,
    /// "stash before the <leg> skipped" for "the home chest ... is not in memory".
    pub gate_chest_unseen: u64,
    /// "home chest at ...": the home chest placed.
    pub home_placed: u64,
    /// The fetch after a respawn: "fetch after the respawn at ...", "... skipped", "... not
    /// needed", "... waits".
    pub fetch_begun: u64,
    pub fetch_skipped: u64,
    pub fetch_not_needed: u64,
    pub fetch_waits: u64,
}

impl ChestCounts {
    /// Counts one note; false when it is none of these.
    fn add(&mut self, what: &str, why: &str) -> bool {
        let slot = if what.starts_with("stash done:") {
            &mut self.stash_done
        } else if what.starts_with("stash failed:") {
            &mut self.stash_failed
        } else if what.starts_with("fetch done:") {
            &mut self.fetch_done
        } else if what.starts_with("fetch failed:") {
            &mut self.fetch_failed
        } else if what.starts_with("home chest at ") {
            &mut self.home_placed
        } else if let Some(rest) = what.strip_prefix("fetch after the respawn ") {
            if rest.starts_with("at ") {
                &mut self.fetch_begun
            } else if rest.starts_with("skipped") {
                &mut self.fetch_skipped
            } else if rest.starts_with("not needed") {
                &mut self.fetch_not_needed
            } else if rest.starts_with("waits") {
                &mut self.fetch_waits
            } else {
                return false;
            }
        } else if let Some(rest) = what.strip_prefix("stash before the ") {
            if rest.ends_with(" skipped: danger") {
                &mut self.gate_danger
            } else if rest.ends_with(" skipped: no home chest") {
                &mut self.gate_no_home
            } else if rest.ends_with(": nothing to put away") {
                &mut self.gate_nothing
            } else if rest.ends_with(" skipped") {
                if why.starts_with("the keep table could not be read") {
                    &mut self.gate_no_keep
                } else {
                    &mut self.gate_chest_unseen
                }
            } else if rest.contains(" at (") {
                &mut self.gate_stashed
            } else {
                return false;
            }
        } else {
            return false;
        };
        *slot += 1;
        true
    }

    fn sum(&mut self, o: &ChestCounts) {
        self.stash_done += o.stash_done;
        self.stash_failed += o.stash_failed;
        self.fetch_done += o.fetch_done;
        self.fetch_failed += o.fetch_failed;
        self.gate_stashed += o.gate_stashed;
        self.gate_nothing += o.gate_nothing;
        self.gate_danger += o.gate_danger;
        self.gate_no_home += o.gate_no_home;
        self.gate_no_keep += o.gate_no_keep;
        self.gate_chest_unseen += o.gate_chest_unseen;
        self.home_placed += o.home_placed;
        self.fetch_begun += o.fetch_begun;
        self.fetch_skipped += o.fetch_skipped;
        self.fetch_not_needed += o.fetch_not_needed;
        self.fetch_waits += o.fetch_waits;
    }

    /// The counts as one cell: `stash 1/0 fetch 0/0 gate s1 n0 d0 h0 k0 u0 home 1 respawn b0 s0 x0 w0`.
    fn text(&self) -> String {
        format!(
            "stash {}/{} fetch {}/{} gate s{} n{} d{} h{} k{} u{} home {} respawn b{} s{} x{} w{}",
            self.stash_done,
            self.stash_failed,
            self.fetch_done,
            self.fetch_failed,
            self.gate_stashed,
            self.gate_nothing,
            self.gate_danger,
            self.gate_no_home,
            self.gate_no_keep,
            self.gate_chest_unseen,
            self.home_placed,
            self.fetch_begun,
            self.fetch_skipped,
            self.fetch_not_needed,
            self.fetch_waits
        )
    }
}

impl KeepCounts {
    fn sum(&mut self, o: &KeepCounts) {
        self.drops += o.drops;
        self.wants += o.wants;
        self.dropped_then_wanted += o.dropped_then_wanted;
        for (k, n) in &o.wants_by_source {
            *self.wants_by_source.entry(k.clone()).or_default() += n;
        }
        for (k, t) in &o.want_list {
            let e = self.want_list.entry(k.clone()).or_default();
            e.lines += t.lines;
            e.n += t.n;
        }
        self.chest.sum(&o.chest);
    }
}

/// The kind a drop line names: "drop Cobblestone from slot 21" is `Cobblestone`, and so are
/// the forest base's "base: drop Cobblestone from slot 21" and "base: a main slot freed: drop
/// Cobblestone from slot 21".
pub fn drop_kind(what: &str) -> Option<&str> {
    let what = what.strip_prefix("base: ").unwrap_or(what);
    let what = what.strip_prefix("a main slot freed: ").unwrap_or(what);
    let mut words = what.strip_prefix("drop ")?.split_whitespace();
    let kind = words.next()?;
    (words.next() == Some("from")).then_some(kind)
}

/// The kinds a want line names: "want Cobblestone|Blackstone 3 for craft stone_pickaxe" is
/// both. "fuel" names no item and matches no drop.
pub fn want_kinds(what: &str) -> Option<Vec<&str>> {
    let mut words = what.strip_prefix("want ")?.split_whitespace();
    let kinds = words.next()?;
    words.next()?.parse::<i64>().ok()?;
    Some(kinds.split('|').collect())
}

/// Where a want line came from: "want Stick 4 for goal kit (base)" is `base`, "... for goal
/// kit" is `goal`, "... for craft bow" is `craft`, "... for smelt IronIngot" is `smelt`.
pub fn want_source(what: &str) -> Option<&str> {
    want_kinds(what)?;
    let (_, rest) = what.split_once(" for ")?;
    if let Some(tag) = rest
        .strip_suffix(')')
        .and_then(|r| r.rsplit_once(" ("))
        .map(|(_, t)| t)
    {
        return Some(tag);
    }
    rest.split_whitespace().next()
}

/// The count a want line asks for: "want Stick 4 for ..." is 4.
fn want_n(what: &str) -> i64 {
    what.split_whitespace()
        .nth(2)
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// The keep counts of one run's ledger.
pub fn keep_counts(l: &Ledger) -> KeepCounts {
    let mut last_drop: BTreeMap<String, u64> = BTreeMap::new();
    let mut c = KeepCounts::default();
    for t in &l.tasks {
        let plan::TaskRecord::Note { at, what, why } = t else {
            continue;
        };
        if let Some(k) = drop_kind(what) {
            c.drops += 1;
            last_drop.insert(k.to_owned(), *at);
        } else if let Some(kinds) = want_kinds(what) {
            c.wants += 1;
            let src = want_source(what).unwrap_or("?").to_owned();
            *c.wants_by_source.entry(src.clone()).or_default() += 1;
            let e = c.want_list.entry((src, kinds.join("|"))).or_default();
            e.lines += 1;
            e.n += want_n(what);
            let hits: Vec<DropWant> = kinds
                .iter()
                .filter_map(|k| {
                    last_drop.get(*k).filter(|d| *d < at).map(|d| DropWant {
                        kind: (*k).to_owned(),
                        dropped_at: *d,
                        wanted_at: *at,
                    })
                })
                .collect();
            if !hits.is_empty() {
                c.dropped_then_wanted += 1;
                c.cases.push(format!("{at} {what}"));
                c.pairs.extend(hits);
            }
        } else {
            c.chest.add(what, why);
        }
    }
    c
}

/// One run's keep counts with where it ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunKeep {
    pub seed: String,
    pub bot: String,
    pub counts: KeepCounts,
}

/// Every run of the batches `batch_files` (each `batch.rsb`, its bots' `<bot>.ledger.rsl`
/// beside it, the seed from the nearest `lanes.toml` above, as `results seeds` reads it). A
/// row with no ledger is reported on stderr and left out.
pub fn runs(batch_files: &[std::path::PathBuf]) -> Result<Vec<RunKeep>, String> {
    let mut out = Vec::new();
    for f in batch_files {
        let b = crate::read_batch(f)?;
        let dir = f.parent().unwrap_or(Path::new("."));
        let seed = crate::seeds::batch_seed(f, b.lane);
        for row in &b.rows {
            let path = dir.join(format!("{}.ledger.rsl", row.bot));
            match crate::read::<Ledger>(&path) {
                Ok(l) => out.push(RunKeep {
                    seed: seed.clone(),
                    bot: row.bot.clone(),
                    counts: keep_counts(&l),
                }),
                Err(e) => eprintln!("keep: {}: {e}", path.display()),
            }
        }
    }
    out.sort_by(|a, b| (&a.seed, &a.bot).cmp(&(&b.seed, &b.bot)));
    Ok(out)
}

/// Wants as one cell: `craft Stick 2x4, goal Cobblestone 1x3` (source kinds lines x count), or -.
fn wants_text(list: &BTreeMap<(String, String), WantTally>) -> String {
    if list.is_empty() {
        return "-".to_owned();
    }
    list.iter()
        .map(|((src, kinds), t)| format!("{src} {kinds} {}x{}", t.lines, t.n))
        .collect::<Vec<_>>()
        .join(", ")
}

fn row_line(seed: &str, bot: &str, c: &KeepCounts) -> String {
    let pairs = if c.pairs.is_empty() {
        "-".to_owned()
    } else {
        c.pairs
            .iter()
            .map(|p| format!("{} dropped {} wanted {}", p.kind, p.dropped_at, p.wanted_at))
            .collect::<Vec<_>>()
            .join("; ")
    };
    format!(
        "| {seed} | {bot} | {} | {} | {} | {} | {pairs} | {} |",
        c.drops,
        c.wants,
        wants_text(&c.want_list),
        c.dropped_then_wanted,
        c.chest.text()
    )
}

/// The table `results keep` prints: a row a run, a total a seed under its runs, then the
/// total. Chest cells: stash done/failed, fetch done/failed, the stash gate (s stashed, n nothing
/// to put away, d danger, h no home chest, k no keep table, u home chest not in memory), home
/// chests placed, the fetch after a respawn (b begun, s skipped, x not needed, w waits).
pub fn render(runs: &[RunKeep]) -> String {
    let mut out = String::new();
    out.push_str(
        "## keep: drops, wants and dropped-then-wanted (issue 77; the bar is 0)

",
    );
    out.push_str("| seed | bot | drops | wants | wants by source (source kinds lines x count) | dropped-then-wanted | dropped then wanted (kind, ticks) | chest |
");
    out.push_str(
        "|---|---|---|---|---|---|---|---|
",
    );
    let mut total = KeepCounts::default();
    let mut seeds: Vec<&str> = runs.iter().map(|r| r.seed.as_str()).collect();
    seeds.dedup();
    for seed in seeds {
        let mut sum = KeepCounts::default();
        let mut n = 0;
        for r in runs.iter().filter(|r| r.seed == seed) {
            out.push_str(&row_line(seed, &r.bot, &r.counts));
            out.push('\n');
            sum.sum(&r.counts);
            n += 1;
        }
        out.push_str(&row_line(seed, &format!("**{n} runs**"), &sum));
        out.push('\n');
        total.sum(&sum);
    }
    out.push('\n');
    out.push_str(&format!(
        "total: {} runs, {} drops, {} wants, dropped-then-wanted {}{}\n",
        runs.len(),
        total.drops,
        total.wants,
        total.dropped_then_wanted,
        if total.dropped_then_wanted == 0 {
            ""
        } else {
            " !!"
        }
    ));
    out.push_str(&format!("total chest: {}\n", total.chest.text()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(at: u64, what: &str) -> plan::TaskRecord {
        plan::TaskRecord::Note {
            at,
            what: what.to_owned(),
            why: String::new(),
        }
    }

    fn ledger(tasks: Vec<plan::TaskRecord>) -> Ledger {
        Ledger {
            format_version: 0,
            run: "r".to_owned(),
            bot: "b".to_owned(),
            commit: "c".to_owned(),
            tasks,
            dropped: 0,
            facts: Vec::new(),
        }
    }

    fn note_why(at: u64, what: &str, why: &str) -> plan::TaskRecord {
        plan::TaskRecord::Note {
            at,
            what: what.to_owned(),
            why: why.to_owned(),
        }
    }

    /// The stash and fetch notes as stash.rs writes them are each counted once, in their cell.
    #[test]
    fn the_stash_paths_are_counted() {
        let l = ledger(vec![
            note(1, "home chest at (3, 64, 5)"),
            note(2, "stash before the nether at (3, 64, 5): 64 cobblestone"),
            note(3, "stash done: 64 items at (3, 64, 5)"),
            note(4, "stash before the end: nothing to put away"),
            note(5, "stash before the end skipped: danger"),
            note(6, "stash before the end skipped: no home chest"),
            note_why(
                7,
                "stash before the end skipped",
                "the keep table could not be read: x",
            ),
            note_why(
                8,
                "stash before the end skipped",
                "the home chest (1, 2, 3) is not in memory",
            ),
            note(9, "fetch after the respawn at (3, 64, 5): 64 cobblestone"),
            note(10, "fetch done: 64 items at (3, 64, 5)"),
            note(11, "fetch failed: StaleWindow, 0 items moved"),
            note(12, "fetch after the respawn skipped"),
            note(13, "fetch after the respawn not needed"),
            note(14, "fetch after the respawn waits"),
            note(15, "stash failed: Unopened, 0 items moved"),
        ]);
        let c = keep_counts(&l).chest;
        assert_eq!(
            c,
            ChestCounts {
                stash_done: 1,
                stash_failed: 1,
                fetch_done: 1,
                fetch_failed: 1,
                gate_stashed: 1,
                gate_nothing: 1,
                gate_danger: 1,
                gate_no_home: 1,
                gate_no_keep: 1,
                gate_chest_unseen: 1,
                home_placed: 1,
                fetch_begun: 1,
                fetch_skipped: 1,
                fetch_not_needed: 1,
                fetch_waits: 1,
            }
        );
    }

    /// The table: a row a run with its wants by source and kind and each dropped-then-wanted
    /// kind with both ticks, a total a seed, then the total, flagged when over 0.
    #[test]
    fn the_keep_table_shows_each_run_a_seed_and_the_total() {
        let a = keep_counts(&ledger(vec![
            note(20, "drop Cobblestone from slot 12"),
            note(30, "base: drop Cobblestone from slot 13"),
            note(
                40,
                "want Cobblestone|Blackstone 3 for goal stone-pickaxe (stronghold)",
            ),
            note(50, "want Stick 2 for craft stone_pickaxe"),
            note(60, "want Stick 2 for craft stone_pickaxe"),
            note(70, "stash done: 3 items at (0, 64, 0)"),
        ]));
        assert_eq!(
            a.pairs,
            vec![DropWant {
                kind: "Cobblestone".to_owned(),
                dropped_at: 30,
                wanted_at: 40,
            }]
        );
        assert_eq!(
            a.want_list.get(&("craft".to_owned(), "Stick".to_owned())),
            Some(&WantTally { lines: 2, n: 4 })
        );
        let b = keep_counts(&ledger(vec![note(5, "want fuel 1 for smelt IronIngot")]));
        let runs = vec![
            RunKeep {
                seed: "cold".to_owned(),
                bot: "rs-l00-b00".to_owned(),
                counts: a,
            },
            RunKeep {
                seed: "cold".to_owned(),
                bot: "rs-l00-b01".to_owned(),
                counts: b.clone(),
            },
            RunKeep {
                seed: "pale-garden".to_owned(),
                bot: "rs-l01-b00".to_owned(),
                counts: b,
            },
        ];
        let md = render(&runs);
        assert!(
            md.contains("| cold | rs-l00-b00 | 2 | 3 | craft Stick 2x4, stronghold Cobblestone|Blackstone 1x3 | 1 | Cobblestone dropped 30 wanted 40 | stash 1/0 "),
            "{md}"
        );
        assert!(md.contains("| cold | **2 runs** | 2 | 4 |"), "{md}");
        assert!(
            md.contains("| pale-garden | **1 runs** | 0 | 1 | smelt fuel 1x1 | 0 | - |"),
            "{md}"
        );
        assert!(
            md.contains("total: 3 runs, 2 drops, 5 wants, dropped-then-wanted 1 !!"),
            "{md}"
        );
    }

    #[test]
    fn drop_and_want_lines_are_read() {
        assert_eq!(
            drop_kind("drop Cobblestone from slot 21"),
            Some("Cobblestone")
        );
        assert_eq!(drop_kind("drop String from the cursor"), Some("String"));
        assert_eq!(drop_kind("drop the plan"), None);
        assert_eq!(drop_kind("base: drop Dirt from slot 22"), Some("Dirt"));
        assert_eq!(
            drop_kind("base: a main slot freed: drop Netherrack from slot 9"),
            Some("Netherrack")
        );
        assert_eq!(
            want_kinds("want OakPlanks|SprucePlanks 4 for goal kit"),
            Some(vec!["OakPlanks", "SprucePlanks"])
        );
        assert_eq!(
            want_kinds("want Cobblestone|Blackstone 3 for craft stone_pickaxe"),
            Some(vec!["Cobblestone", "Blackstone"])
        );
        assert_eq!(
            want_kinds("want fuel 1 for smelt IronIngot"),
            Some(vec!["fuel"])
        );
        assert_eq!(want_kinds("want to go home"), None);
        assert_eq!(
            want_source("want Stick 4 for goal kit (base)"),
            Some("base")
        );
        assert_eq!(
            want_source("want Stick 4 for goal end-kit-fuel (end_portal)"),
            Some("end_portal")
        );
        assert_eq!(want_source("want Stick 4 for goal kit"), Some("goal"));
        assert_eq!(want_source("want String 1 for craft bow"), Some("craft"));
        assert_eq!(
            want_source("want fuel 1 for smelt IronIngot"),
            Some("smelt")
        );
        assert_eq!(want_source("drop Dirt from slot 9"), None);
    }

    /// A want of a kind dropped earlier counts; one of a kind never dropped, or dropped only
    /// later, does not.
    #[test]
    fn a_want_after_its_kinds_drop_counts() {
        let l = ledger(vec![
            note(10, "want String 1 for craft bow"),
            note(20, "drop String from slot 12"),
            note(30, "drop Cobblestone from the cursor"),
            note(40, "want Cobblestone|Blackstone 3 for craft stone_pickaxe"),
            note(50, "want Stick 2 for craft stone_pickaxe"),
            note(60, "want fuel 1 for smelt IronIngot"),
        ]);
        let c = keep_counts(&l);
        assert_eq!((c.drops, c.wants, c.dropped_then_wanted), (2, 4, 1));
        assert_eq!(
            c.wants_by_source,
            BTreeMap::from([("craft".to_owned(), 3), ("smelt".to_owned(), 1)])
        );
        assert_eq!(
            c.cases,
            vec!["40 want Cobblestone|Blackstone 3 for craft stone_pickaxe".to_owned()]
        );
    }
}
