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
    /// Want lines whose kind was last lost to a death, not dropped: a death clears the drop
    /// memory, so these are never dropped-then-wanted (lead-77, race 46140744043: all six
    /// dropped-then-wanted there had a death between the drop and the want).
    #[serde(default)]
    pub wanted_after_death: u64,
    /// Deaths seen (the `Death` fact or the `died (...) and respawned` note, once a tick).
    #[serde(default)]
    pub deaths: u64,
    /// Drops that left less than the kind's keep count (read from the item fact after the drop
    /// and the keep table at the drop line's stage and contexts): to be 0.
    #[serde(default)]
    pub under_keep: Vec<UnderKeep>,
}

/// A drop that left less of its kind than the keep table keeps there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnderKeep {
    pub kind: String,
    pub at: u64,
    pub left: i32,
    pub keep: i32,
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
    /// The home chest crafted early (wall 5, `Plan::early_chest`): "home chest crafted early,
    /// N planks spare" and "home chest early craft skipped" notes; its tasks (the
    /// `carry-chest` row's steps, and the table dug in the tick the chest was held) and the
    /// server ticks they ran: the early craft's cost.
    #[serde(default)]
    pub early_crafted: u64,
    #[serde(default)]
    pub early_skipped: u64,
    #[serde(default)]
    pub early_tasks: u64,
    #[serde(default)]
    pub early_ticks: u64,
    /// The stocking's extra logs for the chest ("stock: +N logs for the home chest (held L,
    /// want W)"): the N summed, and the ticks from that note to the next "home chest crafted
    /// early" (the gather the logs added; none when the chest was never crafted).
    #[serde(default)]
    pub early_logs: u64,
    #[serde(default)]
    pub early_log_ticks: u64,
}

/// The reason `Plan::early_chest` gives each step of the `carry-chest` row (stash.rs).
pub const EARLY_WHY: &str = "the wood spares before the portal: the home chest crafted early";

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
        } else if what.starts_with("home chest crafted early") {
            &mut self.early_crafted
        } else if what.starts_with("home chest early craft skipped") {
            &mut self.early_skipped
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
        self.early_crafted += o.early_crafted;
        self.early_skipped += o.early_skipped;
        self.early_tasks += o.early_tasks;
        self.early_ticks += o.early_ticks;
        self.early_logs += o.early_logs;
        self.early_log_ticks += o.early_log_ticks;
    }

    /// The early craft's tasks and their ticks, from the task records: each started with the
    /// early craft's reason, and the table dug in a tick the chest was held ("dig the table ..."
    /// begun with "home chest crafted early" that tick). A task still open at the end has no
    /// ticks.
    fn early_cost(&mut self, tasks: &[plan::TaskRecord]) {
        let crafted: std::collections::BTreeSet<u64> = tasks
            .iter()
            .filter_map(|t| match t {
                plan::TaskRecord::Note { at, what, .. }
                    if what.starts_with("home chest crafted early") =>
                {
                    Some(*at)
                }
                plan::TaskRecord::Note { .. }
                | plan::TaskRecord::Started { .. }
                | plan::TaskRecord::Ended { .. } => None,
            })
            .collect();
        let ids: std::collections::BTreeSet<u64> = tasks
            .iter()
            .filter_map(|t| match t {
                plan::TaskRecord::Started {
                    id, at, what, why, ..
                } => {
                    let early = why.as_deref().is_some_and(|w| w.starts_with(EARLY_WHY));
                    let dig = crafted.contains(at)
                        && what
                            .as_deref()
                            .is_some_and(|w| w.starts_with("dig the table"));
                    (early || dig).then_some(*id)
                }
                plan::TaskRecord::Ended { .. } | plan::TaskRecord::Note { .. } => None,
            })
            .collect();
        // The extra logs: each note's N, and its ticks to the next crafted-early note.
        let mut since: Option<u64> = None;
        for t in tasks {
            let plan::TaskRecord::Note { at, what, .. } = t else {
                continue;
            };
            if let Some(n) = what
                .strip_prefix("stock: +")
                .and_then(|r| r.split_once(" logs for the home chest"))
                .and_then(|(n, _)| n.parse::<u64>().ok())
            {
                self.early_logs += n;
                since = Some(*at);
            } else if what.starts_with("home chest crafted early")
                && let Some(from) = since.take()
            {
                self.early_log_ticks += at.saturating_sub(from);
            }
        }
        self.early_tasks += ids.len() as u64;
        self.early_ticks += tasks
            .iter()
            .filter_map(|t| match t {
                plan::TaskRecord::Ended { id, ticks, .. } if ids.contains(id) => Some(*ticks),
                plan::TaskRecord::Ended { .. }
                | plan::TaskRecord::Started { .. }
                | plan::TaskRecord::Note { .. } => None,
            })
            .sum::<u64>();
    }

    /// The counts as one cell: `stash 1/0 fetch 0/0 gate s1 n0 d0 h0 k0 u0 home 1 respawn b0 s0
    /// x0 w0 early c1 k0 t3/420 +logs 2/300` (early: crafted, skipped, its tasks / their ticks, the
    /// stocking's extra logs / the ticks they added).
    fn text(&self) -> String {
        format!(
            "stash {}/{} fetch {}/{} gate s{} n{} d{} h{} k{} u{} home {} respawn b{} s{} x{} w{} early c{} k{} t{}/{} +logs {}/{}",
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
            self.fetch_waits,
            self.early_crafted,
            self.early_skipped,
            self.early_tasks,
            self.early_ticks,
            self.early_logs,
            self.early_log_ticks
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
        self.wanted_after_death += o.wanted_after_death;
        self.deaths += o.deaths;
        self.under_keep.extend(o.under_keep.iter().cloned());
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

/// Ticks after a death in which an item's fall is the death's loss: the respawn's inventory
/// comes within a second (race 46140744043 pale b12: died 46680, cobblestone 64 -> 0 at 46700).
pub const DEATH_LOSS_TICKS: u64 = 100;
/// Ticks after a drop line in which its kind's item fact is the drop's (the drop's click is
/// answered within a round trip; the planner's drop clock is `EQUIP_TICKS`).
pub const DROP_SETTLE_TICKS: u64 = 40;

/// A ledger item name ("cobblestone") as the drop and want lines name it ("Cobblestone").
fn line_name(item: &str) -> Option<String> {
    data::ItemKind::from_name(item.strip_prefix("minecraft:").unwrap_or(item))
        .map(|k| format!("{k:?}"))
}

/// The keep count a drop line's reason says held there: its stage and contexts, read against
/// the keep table ("...; needed: 64 by win; stage nether; always+home_leg"). None for the old
/// lists' fallback or a reason without them.
fn keep_at(table: &plan::keep::Keep, kind: &str, why: &str) -> Option<i32> {
    let parts: Vec<&str> = why.split("; ").collect();
    let stage = parts.iter().find_map(|p| p.strip_prefix("stage "))?;
    let reached = world::Stage::from_name(stage);
    let ctx: Vec<plan::keep::Context> = parts
        .last()?
        .split('+')
        .filter_map(|n| plan::keep::Context::ALL.into_iter().find(|c| c.name() == n))
        .collect();
    if ctx.is_empty() {
        return None;
    }
    table
        .junk(reached, &ctx)
        .into_iter()
        .find(|(k, _)| format!("{k:?}") == kind)
        .map(|(_, n)| n)
}

/// One thing on a run's timeline, notes and facts merged by tick.
enum Ev<'a> {
    Death,
    Item { item: &'a str, from: i32, to: i32 },
    Note { what: &'a str, why: &'a str },
}

/// The keep counts of one run's ledger: its notes and its facts on one timeline. A death clears
/// the drop memory (its kinds, and those it took, are then lost to the death, not dropped).
pub fn keep_counts(l: &Ledger) -> KeepCounts {
    let table = plan::keep::Keep::parse(plan::keep::KEEP_TOML).ok();
    let mut evs: Vec<(u64, u8, Ev)> = Vec::new();
    for t in &l.tasks {
        if let plan::TaskRecord::Note { at, what, why } = t {
            evs.push((*at, 1, Ev::Note { what, why }));
        }
    }
    for f in &l.facts {
        match &f.fact {
            observe::Fact::Death => evs.push((f.at, 0, Ev::Death)),
            observe::Fact::Item { item, from, to } => evs.push((
                f.at,
                0,
                Ev::Item {
                    item,
                    from: *from,
                    to: *to,
                },
            )),
            _ => {}
        }
    }
    // By tick, facts before the notes of the same tick; stable within each.
    evs.sort_by_key(|(at, order, _)| (*at, *order));
    let mut last_drop: BTreeMap<String, u64> = BTreeMap::new();
    let mut lost: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut death_at: Option<u64> = None;
    // Drops waiting for their item fact: kind, tick, the keep there.
    let mut pending: Vec<(String, u64, i32)> = Vec::new();
    let mut c = KeepCounts::default();
    for (at, _, ev) in evs {
        pending.retain(|(_, d, _)| at <= d + DROP_SETTLE_TICKS);
        match ev {
            Ev::Death => {
                if death_at != Some(at) {
                    c.deaths += 1;
                }
                death_at = Some(at);
                lost.extend(std::mem::take(&mut last_drop).into_keys());
                pending.clear();
            }
            Ev::Item { item, from, to } => {
                let Some(name) = line_name(item) else {
                    continue;
                };
                if to < from && death_at.is_some_and(|d| at <= d + DEATH_LOSS_TICKS) {
                    last_drop.remove(&name);
                    lost.insert(name.clone());
                }
                if to < from
                    && let Some(i) = pending.iter().position(|(k, _, _)| *k == name)
                {
                    let (kind, d, keep) = pending.remove(i);
                    if to < keep {
                        c.under_keep.push(UnderKeep {
                            kind,
                            at: d,
                            left: to,
                            keep,
                        });
                    }
                }
            }
            Ev::Note { what, why } => {
                if what.starts_with("died (") && what.ends_with(") and respawned") {
                    if death_at.is_none_or(|d| at > d + DEATH_LOSS_TICKS) {
                        c.deaths += 1;
                        death_at = Some(at);
                    }
                    lost.extend(std::mem::take(&mut last_drop).into_keys());
                    pending.clear();
                } else if let Some(k) = drop_kind(what) {
                    c.drops += 1;
                    last_drop.insert(k.to_owned(), at);
                    lost.remove(k);
                    // The base's drop has its own gate (whole stacks of its list): no keep.
                    if !what.starts_with("base: ")
                        && let Some(keep) = table.as_ref().and_then(|t| keep_at(t, k, why))
                    {
                        pending.push((k.to_owned(), at, keep));
                    }
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
                            last_drop.get(*k).filter(|d| **d < at).map(|d| DropWant {
                                kind: (*k).to_owned(),
                                dropped_at: *d,
                                wanted_at: at,
                            })
                        })
                        .collect();
                    if !hits.is_empty() {
                        c.dropped_then_wanted += 1;
                        c.cases.push(format!("{at} {what}"));
                        c.pairs.extend(hits);
                    } else if kinds.iter().any(|k| lost.contains(*k)) {
                        c.wanted_after_death += 1;
                    }
                } else {
                    c.chest.add(what, why);
                }
            }
        }
    }
    c.chest.early_cost(&l.tasks);
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
    let under = if c.under_keep.is_empty() {
        "0".to_owned()
    } else {
        format!(
            "{}: {}",
            c.under_keep.len(),
            c.under_keep
                .iter()
                .map(|u| format!("{} {}: {}/{}", u.kind, u.at, u.left, u.keep))
                .collect::<Vec<_>>()
                .join("; ")
        )
    };
    format!(
        "| {seed} | {bot} | {} | {} | {} | {} | {pairs} | {} | {} | {under} | {} |",
        c.drops,
        c.wants,
        wants_text(&c.want_list),
        c.dropped_then_wanted,
        c.deaths,
        c.wanted_after_death,
        c.chest.text()
    )
}

/// The table `results keep` prints: a row a run, a total a seed under its runs, then the
/// total. Chest cells: stash done/failed, fetch done/failed, the stash gate (s stashed, n nothing
/// to put away, d danger, h no home chest, k no keep table, u home chest not in memory), home
/// chests placed, the fetch after a respawn (b begun, s skipped, x not needed, w waits), the
/// early craft (c crafted, k skipped, t its tasks / their ticks, +logs the stocking's extra logs /
/// the ticks they added).
pub fn render(runs: &[RunKeep]) -> String {
    let mut out = String::new();
    out.push_str("## keep: drops, wants and dropped-then-wanted (issue 77; the bar is 0)\n\n");
    out.push_str(concat!(
        "| seed | bot | drops | wants | wants by source (source kinds lines x count) ",
        "| dropped-then-wanted | dropped then wanted (kind, ticks) | deaths ",
        "| wanted after a death | drops under keep (kind at: left/keep) | chest |\n",
        "|---|---|---|---|---|---|---|---|---|---|---|\n",
    ));
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
    let flag = |bad: bool| if bad { " !!" } else { "" };
    out.push('\n');
    out.push_str(&format!(
        "total: {} runs, {} drops, {} wants, dropped-then-wanted {}{}\n",
        runs.len(),
        total.drops,
        total.wants,
        total.dropped_then_wanted,
        flag(total.dropped_then_wanted > 0)
    ));
    out.push_str(&format!(
        "total: {} deaths, {} wanted after a death, drops under keep {}{}\n",
        total.deaths,
        total.wanted_after_death,
        total.under_keep.len(),
        flag(!total.under_keep.is_empty())
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

    fn item(at: u64, item: &str, from: i32, to: i32) -> observe::Stamped {
        observe::Stamped {
            at,
            fact: observe::Fact::Item {
                item: item.to_owned(),
                from,
                to,
            },
        }
    }

    const WHY: &str = "fewer than 6 main slots free; needed: 64 by win; stage nether; always";

    /// Drop K, die, want K: the death took it, not the drop (race 46140744043's six): 0
    /// dropped-then-wanted, 1 wanted after a death. A drop of K after the death counts again.
    #[test]
    fn a_want_after_a_death_is_not_blamed_on_the_drop() {
        let mut l = ledger(vec![
            note_why(100, "drop Cobblestone from slot 12", WHY),
            note(200, "died (Warden) and respawned"),
            note(
                300,
                "want Cobblestone|Blackstone 3 for goal stone-pickaxe (stronghold)",
            ),
        ]);
        l.facts = vec![
            observe::Stamped {
                at: 180,
                fact: observe::Fact::Death,
            },
            item(190, "cobblestone", 64, 0),
        ];
        let c = keep_counts(&l);
        assert_eq!(
            (c.dropped_then_wanted, c.wanted_after_death, c.deaths),
            (0, 1, 1)
        );
        l.tasks
            .push(note_why(400, "drop Cobblestone from slot 12", WHY));
        l.tasks
            .push(note(500, "want Cobblestone 1 for craft furnace"));
        let c = keep_counts(&l);
        assert_eq!((c.dropped_then_wanted, c.wanted_after_death), (1, 1));
        // Death with no drop before it: what it took is still "after a death".
        let mut d = ledger(vec![note(300, "want Stick 2 for craft stone_pickaxe")]);
        d.facts = vec![
            observe::Stamped {
                at: 100,
                fact: observe::Fact::Death,
            },
            item(120, "stick", 4, 0),
        ];
        assert_eq!(keep_counts(&d).wanted_after_death, 1);
    }

    /// A drop that leaves less than the keep there (its line's stage and contexts read against
    /// keep.toml) is reported with what it left; one that leaves the keep is not. The base's
    /// drops have their own gate and are never checked.
    #[test]
    fn a_drop_under_its_keep_is_reported() {
        let mut l = ledger(vec![
            note_why(100, "drop Cobblestone from slot 12", WHY),
            note_why(200, "drop Cobblestone from slot 13", WHY),
            note(300, "base: drop Dirt from slot 14"),
        ]);
        l.facts = vec![
            item(110, "cobblestone", 128, 64),
            item(210, "cobblestone", 64, 0),
            item(310, "dirt", 30, 0),
        ];
        let c = keep_counts(&l);
        assert_eq!(
            c.under_keep,
            vec![UnderKeep {
                kind: "Cobblestone".to_owned(),
                at: 200,
                left: 0,
                keep: 64,
            }]
        );
        let md = render(&[RunKeep {
            seed: "cold".to_owned(),
            bot: "b".to_owned(),
            counts: c,
        }]);
        assert!(md.contains("| 1: Cobblestone 200: 0/64 |"), "{md}");
        assert!(md.contains("drops under keep 1 !!"), "{md}");
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
                ..ChestCounts::default()
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
            md.contains("| cold | rs-l00-b00 | 2 | 3 | craft Stick 2x4, stronghold Cobblestone|Blackstone 1x3 | 1 | Cobblestone dropped 30 wanted 40 | 0 | 0 | 0 | stash 1/0 "),
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

    fn started(id: u64, at: u64, task: &str, what: &str, why: &str) -> plan::TaskRecord {
        plan::TaskRecord::Started {
            id,
            at,
            task: task.to_owned(),
            what: Some(what.to_owned()),
            why: Some(why.to_owned()),
        }
    }

    fn ended(id: u64, at: u64, task: &str, ticks: u64) -> plan::TaskRecord {
        plan::TaskRecord::Ended {
            id,
            at,
            task: task.to_owned(),
            ticks,
            outcome: None,
            why: None,
        }
    }

    /// Wall 5's early craft: its notes counted, and its cost the ticks of its own tasks (the
    /// carry-chest steps and the table dug as the chest was held), never another task's.
    #[test]
    fn the_early_craft_and_its_cost_are_counted() {
        let early = format!("{EARLY_WHY}, carried; task 1 of at most 8");
        let l = ledger(vec![
            note_why(
                5,
                "home chest early craft skipped",
                "craft:chest would be a trip",
            ),
            note(60, "stock: +2 logs for the home chest (held 4, want 6)"),
            started(
                1,
                10,
                "walk-to-log",
                "walk to log (1, 2, 3)",
                "the nearest log",
            ),
            ended(1, 40, "walk-to-log", 30),
            note_why(100, "home chest: place-table", &early),
            started(2, 100, "place", "home chest: place-table", &early),
            ended(2, 104, "place", 4),
            started(3, 105, "craft", "home chest: craft:chest", &early),
            ended(3, 125, "craft", 20),
            note(126, "home chest crafted early, 16 planks spare"),
            note(126, "dig the table (1, 64, 0)"),
            started(
                4,
                126,
                "dig",
                "dig the table (1, 64, 0)",
                "take the table along",
            ),
            ended(4, 150, "dig", 24),
            // A later table dig is the planner's own, not the early craft's.
            started(
                5,
                900,
                "dig",
                "dig the table (1, 64, 0)",
                "take the table along",
            ),
            ended(5, 920, "dig", 20),
        ]);
        let c = keep_counts(&l).chest;
        assert_eq!(
            (
                c.early_crafted,
                c.early_skipped,
                c.early_tasks,
                c.early_ticks
            ),
            (1, 1, 3, 48)
        );
        // The extra logs: 2, from the note at 60 to the chest crafted at 126.
        assert_eq!((c.early_logs, c.early_log_ticks), (2, 66));
        assert!(
            c.text().ends_with("early c1 k1 t3/48 +logs 2/66"),
            "{}",
            c.text()
        );
        let mut total = ChestCounts::default();
        total.sum(&c);
        total.sum(&c);
        assert_eq!((total.early_tasks, total.early_ticks), (6, 96));
    }
}
