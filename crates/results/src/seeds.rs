//! Issue 69's per-seed time, read from saved worlds on a runner (never on the Mac, never in the
//! bot: a world file is development knowledge, no X-ray). A snapshot directory is `snap.toml`
//! beside `world/level.dat`; the world's clock (`Data.Time`, game ticks since the world was made)
//! rides along every copy, so a snapshot resumed from another keeps counting from the first
//! spawn. Per seed: the furthest stage and the game ticks from spawn to each stage; then one table,
//! seeds as rows and stages as columns.
//!
//! A lineage (`config`-style TOML, [`Lineage`]) names the snapshots one world went through in
//! order, so the chain a frontier was built from (the issue-20 win) reads as one row.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use protocol::bytes::Reader;
use protocol::nbt::{self, Nbt};
use serde::{Deserialize, Serialize};
use world::Stage;

/// The seed of a snapshot whose `snap.toml` names none: `config/race.toml`'s `[server] seed`,
/// the only seed every race ran before issue 69.
pub const DEFAULT_SEED: &str = "typecraft";

/// Game ticks a second (vanilla's fixed 20 TPS: `MinecraftServer` ticks every 50 ms).
const TICKS_PER_SECOND: i64 = 20;

/// A named root (a file's NBT) read as the network's anonymous root.
pub fn named_root(raw: &[u8]) -> Result<Nbt, String> {
    let (&tag, rest) = raw.split_first().ok_or("an empty file")?;
    if tag != 10 {
        return Err(format!("the root tag is {tag}, not a compound"));
    }
    let len = match rest {
        [a, b, ..] => usize::from(u16::from_be_bytes([*a, *b])),
        _ => return Err("no root name".to_owned()),
    };
    let body = rest
        .get(2 + len..)
        .ok_or("the root name runs past the end")?;
    let anon: Vec<u8> = std::iter::once(10).chain(body.iter().copied()).collect();
    nbt::read_root(&mut Reader::new(&anon)).map_err(|e| format!("{e:?}"))
}

/// An NBT file, gunzipped when it starts with gzip's magic (level.dat and `data/*.dat` are).
pub fn read_nbt(path: &Path) -> Result<Nbt, String> {
    let raw = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bytes = if raw.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(raw.as_slice())
            .read_to_end(&mut out)
            .map_err(|e| format!("{}: gunzip: {e}", path.display()))?;
        out
    } else {
        raw
    };
    named_root(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn number(v: &Nbt) -> Option<i64> {
    match v {
        Nbt::Long(n) => Some(*n),
        Nbt::Int(n) => Some(i64::from(*n)),
        Nbt::Short(n) => Some(i64::from(*n)),
        Nbt::Byte(n) => Some(i64::from(*n)),
        _ => None,
    }
}

/// Every whole-number entry whose key `keep` accepts, by its dotted path, in file order.
pub fn find_numbers(root: &Nbt, keep: fn(&str) -> bool) -> Vec<(String, i64)> {
    fn walk(v: &Nbt, path: &str, keep: fn(&str) -> bool, out: &mut Vec<(String, i64)>) {
        match v {
            Nbt::Compound(entries) => {
                for (k, e) in entries {
                    let p = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    if keep(k)
                        && let Some(n) = number(e)
                    {
                        out.push((p.clone(), n));
                    }
                    walk(e, &p, keep, out);
                }
            }
            Nbt::List(items) => {
                for (i, e) in items.iter().enumerate() {
                    walk(e, &format!("{path}[{i}]"), keep, out);
                }
            }
            Nbt::Byte(_)
            | Nbt::Short(_)
            | Nbt::Int(_)
            | Nbt::Long(_)
            | Nbt::Float(_)
            | Nbt::Double(_)
            | Nbt::ByteArray(_)
            | Nbt::String(_)
            | Nbt::IntArray(_)
            | Nbt::LongArray(_) => {}
        }
    }
    let mut out = Vec::new();
    walk(root, "", keep, &mut out);
    out
}

fn is_seed_key(k: &str) -> bool {
    k.to_ascii_lowercase().contains("seed")
}

fn is_time_key(k: &str) -> bool {
    k.to_ascii_lowercase().contains("time")
}

/// What a world's files say of its clock and seed.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Level {
    /// `Data.Time` of level.dat: game ticks since the world was made.
    pub time: Option<i64>,
    /// Every number keyed like a seed, by file and path (`level.dat:Data.WorldGenSettings.seed`
    /// in 1.21; 26.1.2 may keep it elsewhere: measured, not assumed).
    pub seeds: Vec<(String, i64)>,
    /// Every number keyed like a time, by file and path (Time, DayTime, clocks).
    pub times: Vec<(String, i64)>,
}

/// Every `.dat` under a directory, sorted.
fn dats_under(dir: &Path) -> Vec<PathBuf> {
    fn walk(p: &Path, out: &mut Vec<PathBuf>) {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                for e in rd.flatten() {
                    walk(&e.path(), out);
                }
            }
        } else if p.extension().is_some_and(|x| x == "dat") {
            out.push(p.to_path_buf());
        }
    }
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort();
    out
}

/// A world's clock and seed from its level.dat and the `data/` files beside it (where a seed or a
/// clock may have moved). A `data/` file that does not read is said on stderr and skipped.
pub fn level(level_dat: &Path) -> Result<Level, String> {
    let root = read_nbt(level_dat)?;
    let time = root
        .get("Data")
        .and_then(|d| d.get("Time"))
        .and_then(number);
    let mut seeds: Vec<(String, i64)> = find_numbers(&root, is_seed_key)
        .into_iter()
        .map(|(p, n)| (format!("level.dat:{p}"), n))
        .collect();
    let mut times: Vec<(String, i64)> = find_numbers(&root, is_time_key)
        .into_iter()
        .map(|(p, n)| (format!("level.dat:{p}"), n))
        .collect();
    let world = level_dat.parent().unwrap_or(Path::new("."));
    for f in dats_under(&world.join("data")) {
        let name = f
            .strip_prefix(world)
            .unwrap_or(&f)
            .to_string_lossy()
            .into_owned();
        match read_nbt(&f) {
            Ok(r) => {
                seeds.extend(
                    find_numbers(&r, is_seed_key)
                        .into_iter()
                        .map(|(p, n)| (format!("{name}:{p}"), n)),
                );
                times.extend(
                    find_numbers(&r, is_time_key)
                        .into_iter()
                        .map(|(p, n)| (format!("{name}:{p}"), n)),
                );
            }
            Err(e) => eprintln!("level: skipped {e}"),
        }
    }
    Ok(Level { time, seeds, times })
}

/// `results level-time`: the clock first, then every seed and time found.
pub fn render_level(l: &Level) -> String {
    let mut out = String::new();
    match l.time {
        Some(t) => {
            let _ = writeln!(out, "Data.Time {t} ({})", clock(t));
        }
        None => out.push_str("Data.Time absent\n"),
    }
    for (p, n) in &l.seeds {
        let _ = writeln!(out, "seed {p} = {n}");
    }
    for (p, n) in &l.times {
        let _ = writeln!(out, "time {p} = {n}");
    }
    out
}

/// Game ticks as h:mm of game time.
pub fn clock(ticks: i64) -> String {
    let s = ticks / TICKS_PER_SECOND;
    format!("{}:{:02}", s / 3600, (s / 60) % 60)
}

fn commas(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// The `snap.toml` keys this module reads beyond `snapshot::Snap`: issue 69's prepended seed name,
/// and the development and kind marks (`bot::devstart`).
#[derive(Deserialize)]
struct Marks {
    seed: Option<String>,
    #[serde(default)]
    dev: bool,
    kind: Option<String>,
}

/// One snapshot's place on its seed's clock.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapTime {
    pub seed: String,
    pub stage: String,
    /// `Data.Time` at the snapshot: game ticks since the world was made.
    pub ticks: i64,
    pub commit: String,
    pub run: String,
    pub lane: u32,
    pub bot: String,
    /// `at` or `stuck` for a snapshot that is not a stage's.
    #[serde(default)]
    pub kind: Option<String>,
}

/// One snapshot directory read: `None` for a development snapshot (never evidence), said on
/// stderr.
pub fn read_snapshot(dir: &Path) -> Result<Option<SnapTime>, String> {
    let path = dir.join("snap.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let snap: crate::snapshot::Snap =
        toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let marks: Marks = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if marks.dev {
        eprintln!("seeds: left out {} (a development snapshot)", dir.display());
        return Ok(None);
    }
    let lvl = level(&dir.join("world").join("level.dat"))?;
    let ticks = lvl
        .time
        .ok_or_else(|| format!("{}: level.dat has no Data.Time", dir.display()))?;
    Ok(Some(SnapTime {
        seed: marks.seed.unwrap_or_else(|| DEFAULT_SEED.to_owned()),
        stage: snap.stage,
        ticks,
        commit: snap.commit,
        run: snap.run,
        lane: snap.lane,
        bot: snap.bot,
        kind: marks.kind,
    }))
}

/// Every snapshot directory (a `snap.toml` beside `world/level.dat`) under the paths, sorted.
pub fn snapshot_dirs(paths: &[String]) -> Vec<PathBuf> {
    fn walk(p: &Path, out: &mut Vec<PathBuf>) {
        if !p.is_dir() {
            return;
        }
        if p.join("snap.toml").is_file() && p.join("world").join("level.dat").is_file() {
            out.push(p.to_path_buf());
            return;
        }
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                walk(&e.path(), out);
            }
        }
    }
    let mut out = Vec::new();
    for p in paths {
        walk(Path::new(p), &mut out);
    }
    out.sort();
    out
}

/// A lineage: the snapshots one world went through, spawn first (issue 69: the issue-20 win's
/// chain is the table's first row).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lineage {
    pub seed: String,
    #[serde(rename = "link")]
    pub links: Vec<Link>,
}

/// One snapshot of a lineage: where it lives and what it is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    /// The stage its `snap.toml` names (an `at`/`stuck` link names the stage it held).
    pub stage: String,
    /// The release that holds it.
    pub tag: String,
    pub asset: String,
    /// Why it is in the chain (the frontier commit or the `how` that names it).
    #[serde(default)]
    pub note: String,
}

impl Lineage {
    pub fn parse(text: &str) -> Result<Self, String> {
        let l: Lineage = toml::from_str(text).map_err(|e| format!("lineage: {e}"))?;
        if l.links.is_empty() {
            return Err("lineage: no links".to_owned());
        }
        if let Some(bad) = l
            .links
            .iter()
            .find(|k| Stage::from_name(&k.stage).is_none())
        {
            return Err(format!("lineage: {} is not a stage", bad.stage));
        }
        Ok(l)
    }

    pub fn read(path: &Path) -> Result<Self, String> {
        Self::parse(&std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?)
    }

    /// The directory a link's asset is unpacked into under `root`: the asset's name without
    /// `.tar.zst` (each asset its own directory, since every tar's top is its stage's name).
    pub fn link_dir(root: &Path, link: &Link) -> PathBuf {
        root.join(link.asset.trim_end_matches(".tar.zst"))
    }
}

/// Where a stage's ticks came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// A snapshot of that very stage.
    Exact,
    /// No snapshot of it: a later stage's (or an at/stuck link holding a later one), an upper
    /// bound.
    AtMost,
}

/// A seed's clock: per stage reached, the earliest game ticks and where they came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeedRow {
    pub seed: String,
    pub stages: BTreeMap<Stage, (i64, Source)>,
}

impl SeedRow {
    pub fn furthest(&self) -> Option<Stage> {
        self.stages.keys().next_back().copied()
    }
}

/// Fold snapshots into one row a seed: a stage's ticks are the least `Data.Time` among stage
/// snapshots of it; a stage below the furthest with no snapshot takes the least ticks of any
/// snapshot holding a higher stage (an upper bound). An `at`/`stuck` snapshot only bounds.
pub fn fold(snaps: &[SnapTime]) -> Vec<SeedRow> {
    let mut by_seed: BTreeMap<&str, Vec<&SnapTime>> = BTreeMap::new();
    for s in snaps {
        by_seed.entry(s.seed.as_str()).or_default().push(s);
    }
    by_seed
        .into_iter()
        .map(|(seed, list)| {
            let staged: Vec<(Stage, &SnapTime)> = list
                .iter()
                .filter_map(|s| Stage::from_name(&s.stage).map(|st| (st, *s)))
                .collect();
            let top = staged.iter().map(|(st, _)| *st).max();
            let mut stages = BTreeMap::new();
            for st in Stage::ALL
                .into_iter()
                .filter(|st| top.is_some_and(|t| *st <= t))
            {
                let exact = staged
                    .iter()
                    .filter(|(s, t)| *s == st && t.kind.is_none())
                    .map(|(_, t)| t.ticks)
                    .min();
                let bound = staged
                    .iter()
                    .filter(|(s, _)| *s >= st)
                    .map(|(_, t)| t.ticks)
                    .min();
                match (exact, bound) {
                    (Some(e), _) => {
                        stages.insert(st, (e, Source::Exact));
                    }
                    (None, Some(b)) => {
                        stages.insert(st, (b, Source::AtMost));
                    }
                    (None, None) => {}
                }
            }
            SeedRow {
                seed: seed.to_owned(),
                stages,
            }
        })
        .collect()
}

fn cell(v: Option<&(i64, Source)>) -> String {
    match v {
        Some((t, Source::Exact)) => format!("{} ({})", commas(*t), clock(*t)),
        Some((t, Source::AtMost)) => format!("≤{} ({})", commas(*t), clock(*t)),
        None => "-".to_owned(),
    }
}

/// The markdown: per seed its furthest stage and each stage's ticks, then the table.
pub fn render(rows: &[SeedRow]) -> String {
    let mut out = String::from("### seeds: game ticks from spawn to each stage\n\n");
    if rows.is_empty() {
        out.push_str("no snapshots\n");
        return out;
    }
    for r in rows {
        let _ = writeln!(
            out,
            "- **{}**: furthest {}",
            r.seed,
            r.furthest().map_or("none", Stage::name)
        );
        for (st, v) in &r.stages {
            let _ = writeln!(out, "  - {}: {}", st.name(), cell(Some(v)));
        }
    }
    // Columns: the ladder's stages any seed reached, lowest first.
    let cols: Vec<Stage> = Stage::ALL
        .into_iter()
        .filter(|st| rows.iter().any(|r| r.stages.contains_key(st)))
        .collect();
    out.push_str("\n| seed | furthest |");
    for c in &cols {
        let _ = write!(out, " {} |", c.name());
    }
    out.push_str("\n|---|---|");
    out.push_str(&"---|".repeat(cols.len()));
    out.push('\n');
    for r in rows {
        let _ = write!(
            out,
            "| {} | {} |",
            r.seed,
            r.furthest().map_or("none", Stage::name)
        );
        for c in &cols {
            let _ = write!(out, " {} |", cell(r.stages.get(c)));
        }
        out.push('\n');
    }
    out.push_str("\nCells: game ticks (`Data.Time`) and h:mm of game time; ≤ is a bound from a later stage's snapshot.\n");
    out
}

/// One insight-store row: a snapshot's seed, stage and ticks (`ingest` reads these from
/// `seed-rows.toml`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeedRows {
    #[serde(rename = "row", default)]
    pub rows: Vec<SnapTime>,
    /// Each seed's stall and why (issue 69: a seed's wall is recognised by its stage and reason).
    #[serde(default)]
    pub stall: Vec<Stall>,
}

pub fn rows_toml(snaps: &[SnapTime], stalls: &[Stall]) -> Result<String, String> {
    toml::to_string(&SeedRows {
        rows: snaps.iter().filter(|s| s.kind.is_none()).cloned().collect(),
        stall: stalls.to_vec(),
    })
    .map_err(|e| e.to_string())
}

/// One ranked end reason: a signature (`ending killer | doing | why`, `Case::signature`) and the
/// bots at the stall that ended so.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reason {
    pub signature: String,
    pub n: u64,
}

/// Where a seed's bots stopped and why: the furthest stage any counted real run reached, the end
/// reasons of the runs that reached it (most first), and the best of them's last task.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stall {
    pub seed: String,
    pub stage: String,
    /// Counted real runs that reached it, of all counted real runs of the seed.
    pub k: u64,
    pub n: u64,
    pub reasons: Vec<Reason>,
    /// The first to reach it (least stage tick, then name), its race run and lane.
    pub bot: String,
    pub run: String,
    pub lane: u32,
    /// Its ledger's last task, `<task>: <what> (<why>) -> <outcome>`; empty without a ledger.
    #[serde(default)]
    pub last_task: String,
}

/// A ledger's last task started, with its reason and how it ended (`open` while it ran on).
pub fn last_task(l: &crate::Ledger) -> String {
    let Some((id, task, what, why)) = l.tasks.iter().rev().find_map(|t| match t {
        plan::TaskRecord::Started {
            id,
            task,
            what,
            why,
            ..
        } => Some((*id, task, what, why)),
        plan::TaskRecord::Ended { .. } | plan::TaskRecord::Note { .. } => None,
    }) else {
        return String::new();
    };
    let outcome = l
        .tasks
        .iter()
        .find_map(|t| match t {
            plan::TaskRecord::Ended { id: e, outcome, .. } if *e == id => {
                Some(outcome.clone().unwrap_or_else(|| "ended".to_owned()))
            }
            plan::TaskRecord::Started { .. }
            | plan::TaskRecord::Ended { .. }
            | plan::TaskRecord::Note { .. } => None,
        })
        .unwrap_or_else(|| "open".to_owned());
    format!(
        "{task}: {} ({}) -> {outcome}",
        what.as_deref().unwrap_or("-"),
        why.as_deref().unwrap_or("-")
    )
}

/// Each seed's stall from its real batches; `dir` is where a batch's ledgers lie.
pub fn stalls(batches: &[(String, PathBuf, crate::Batch)]) -> Vec<Stall> {
    let mut by_seed: BTreeMap<&str, Vec<(&Path, &crate::Batch, &crate::Row)>> = BTreeMap::new();
    for (seed, dir, b) in batches.iter().filter(|x| x.2.source == crate::Source::Real) {
        for r in b.rows.iter().filter(|r| r.ending.counted()) {
            by_seed
                .entry(seed.as_str())
                .or_default()
                .push((dir.as_path(), b, r));
        }
    }
    by_seed
        .into_iter()
        .filter_map(|(seed, runs)| {
            let top = runs.iter().filter_map(|x| x.2.stage_reached).max()?;
            let at: Vec<&(&Path, &crate::Batch, &crate::Row)> = runs
                .iter()
                .filter(|x| x.2.stage_reached == Some(top))
                .collect();
            let mut counts: BTreeMap<String, u64> = BTreeMap::new();
            for (_, _, r) in &at {
                let [ending, killer, doing, why] = &r.signature;
                *counts
                    .entry(format!("{ending} {killer} | {doing} | {why}"))
                    .or_default() += 1;
            }
            let mut reasons: Vec<Reason> = counts
                .into_iter()
                .map(|(signature, n)| Reason { signature, n })
                .collect();
            reasons.sort_by(|a, b| b.n.cmp(&a.n).then_with(|| a.signature.cmp(&b.signature)));
            let tick = |r: &crate::Row| {
                r.stage_ticks
                    .iter()
                    .find(|t| t.0 == top)
                    .map_or(u64::MAX, |t| t.1)
            };
            let (dir, b, best) = at
                .iter()
                .min_by(|x, y| {
                    tick(x.2)
                        .cmp(&tick(y.2))
                        .then_with(|| x.2.bot.cmp(&y.2.bot))
                })
                .map(|x| **x)?;
            let ledger = dir.join(format!("{}.ledger.rsl", best.bot));
            let last = if ledger.is_file() {
                match crate::read::<crate::Ledger>(&ledger) {
                    Ok(l) => last_task(&l),
                    Err(e) => {
                        eprintln!("seeds: {e}");
                        String::new()
                    }
                }
            } else {
                String::new()
            };
            Some(Stall {
                seed: seed.to_owned(),
                stage: top.name().to_owned(),
                k: at.len() as u64,
                n: runs.len() as u64,
                reasons,
                bot: best.bot.clone(),
                run: b.run.clone(),
                lane: b.lane,
                last_task: last,
            })
        })
        .collect()
}

/// The stalls as markdown: a line a seed, then its ranked reasons and the best bot's last task.
pub fn render_stalls(stalls: &[Stall]) -> String {
    if stalls.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "
### seeds: where the bots stopped

",
    );
    for st in stalls {
        let _ = writeln!(
            out,
            "- **{}**: stall `{}` ({}/{} counted runs reached it)",
            st.seed, st.stage, st.k, st.n
        );
        for r in &st.reasons {
            let _ = writeln!(out, "  - {} x{}", r.signature, r.n);
        }
        let _ = writeln!(
            out,
            "  - best bot {} (race {} lane {}): last task {}",
            st.bot,
            st.run,
            st.lane,
            if st.last_task.is_empty() {
                "unread (no ledger)"
            } else {
                &st.last_task
            }
        );
    }
    out
}

pub fn read_rows(path: &Path) -> Result<SeedRows, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The keys of race.yml's `lanes.toml` read here (69-relay's `seed-relay lanes` writes it): each
/// lane's seed.
#[derive(Deserialize)]
struct LanesToml {
    #[serde(default)]
    lane: Vec<LaneSeed>,
}

#[derive(Deserialize)]
struct LaneSeed {
    lane: u32,
    seed: String,
}

/// A `lanes.toml` as lane → seed.
pub fn read_lanes(path: &Path) -> Result<BTreeMap<u32, String>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let l: LanesToml = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(l.lane.into_iter().map(|s| (s.lane, s.seed)).collect())
}

/// A batch's seed: its lane in the nearest `lanes.toml` at or above its directory (a release
/// unpacked as one tree), else `DEFAULT_SEED` (every race before issue 69).
pub fn batch_seed(batch_path: &Path, lane: u32) -> String {
    batch_path
        .ancestors()
        .skip(1)
        .map(|d| d.join("lanes.toml"))
        .find(|p| p.is_file())
        .and_then(|p| match read_lanes(&p) {
            Ok(m) => m.get(&lane).cloned(),
            Err(e) => {
                eprintln!("seeds: {e}");
                None
            }
        })
        .unwrap_or_else(|| DEFAULT_SEED.to_owned())
}

/// One (seed, commit, start) group's run ticks (issue 56's `Row::stage_ticks`, the session ticks a
/// stage was first reached at): per stage reached after the start, the ticks of the runs that
/// reached it; `n` the counted runs. A stage held from a run's first stage tick (the snapshot's
/// own ladder) is held at the start, not reached, and left out, as `stage_tick_lines` does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunTicks {
    pub n: u64,
    pub reached: BTreeMap<Stage, Vec<u64>>,
}

/// Fold seeded batches into run ticks by (seed, commit, start); sim batches are left out (the
/// real race is the metric).
pub fn run_ticks(
    batches: &[(String, PathBuf, crate::Batch)],
) -> BTreeMap<(String, String, String), RunTicks> {
    let mut out: BTreeMap<(String, String, String), RunTicks> = BTreeMap::new();
    for (seed, _, b) in batches {
        if b.source == crate::Source::Sim {
            continue;
        }
        let g = out
            .entry((seed.clone(), b.commit.clone(), b.start.clone()))
            .or_default();
        for row in b.rows.iter().filter(|r| r.ending.counted()) {
            g.n += 1;
            let first = row.stage_ticks.iter().map(|t| t.1).min();
            for (stage, at) in row.stage_ticks.iter().filter(|t| Some(t.1) != first) {
                g.reached.entry(*stage).or_default().push(*at);
            }
        }
    }
    for g in out.values_mut() {
        for v in g.reached.values_mut() {
            v.sort_unstable();
        }
    }
    out
}

/// The per-seed run ticks table: k/n and the median, p25 and p75 session tick per stage. Empty
/// when no row kept stage ticks.
pub fn render_run_ticks(groups: &BTreeMap<(String, String, String), RunTicks>) -> String {
    if groups.values().all(|g| g.reached.is_empty()) {
        return String::new();
    }
    let mut out = String::from(
        "\n### seeds: run ticks to each stage (session ticks, counted real runs)\n\n| seed | commit | start | stage | k/n | median | p25 | p75 |\n|---|---|---|---|---|---|---|---|\n",
    );
    for ((seed, commit, start), g) in groups {
        let short = commit.get(..7).unwrap_or(commit);
        for (stage, at) in &g.reached {
            let Some([p25, median, p75]) = crate::quartiles(at) else {
                continue;
            };
            let _ = writeln!(
                out,
                "| {seed} | {short} | {start} | {} | {}/{} | {} ({}) | {} | {} |",
                stage.name(),
                at.len(),
                g.n,
                commas(i64::try_from(median).unwrap_or(i64::MAX)),
                clock(i64::try_from(median).unwrap_or(i64::MAX)),
                commas(i64::try_from(p25).unwrap_or(i64::MAX)),
                commas(i64::try_from(p75).unwrap_or(i64::MAX)),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file's bytes: a named root `""` around `v` (write_root's tag, a zero-length name, the
    /// payload).
    fn named(v: &Nbt) -> Vec<u8> {
        let anon = nbt::write_root(v);
        let mut out = vec![10, 0, 0];
        out.extend_from_slice(anon.get(1..).unwrap_or(&[]));
        out
    }

    fn level_nbt(time: i64, seed: i64) -> Nbt {
        Nbt::Compound(vec![(
            "Data".to_owned(),
            Nbt::Compound(vec![
                ("DayTime".to_owned(), Nbt::Long(time % 24_000)),
                ("Time".to_owned(), Nbt::Long(time)),
                (
                    "WorldGenSettings".to_owned(),
                    Nbt::Compound(vec![("seed".to_owned(), Nbt::Long(seed))]),
                ),
            ]),
        )])
    }

    fn gz(bytes: &[u8]) -> Vec<u8> {
        use std::io::Write as _;
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(bytes).expect("gzip");
        e.finish().expect("gzip")
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rs-seeds-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("dir");
        d
    }

    #[test]
    fn a_gzipped_level_dat_gives_its_time_and_seed() {
        let d = tmp("level");
        let world = d.join("world");
        std::fs::create_dir_all(world.join("data").join("minecraft")).expect("dir");
        std::fs::write(
            world.join("level.dat"),
            gz(&named(&level_nbt(466_498, -42))),
        )
        .expect("write");
        // A seed kept in a data file is found there too.
        let extra = Nbt::Compound(vec![("seed".to_owned(), Nbt::Long(7))]);
        std::fs::write(
            world.join("data").join("minecraft").join("gen.dat"),
            gz(&named(&extra)),
        )
        .expect("write");
        let l = level(&world.join("level.dat")).expect("level");
        assert_eq!(l.time, Some(466_498));
        assert_eq!(
            l.seeds,
            vec![
                ("level.dat:Data.WorldGenSettings.seed".to_owned(), -42),
                ("data/minecraft/gen.dat:seed".to_owned(), 7),
            ]
        );
        assert!(
            l.times
                .contains(&("level.dat:Data.Time".to_owned(), 466_498))
        );
        assert!(render_level(&l).starts_with("Data.Time 466498 (6:28)\n"));
        std::fs::remove_dir_all(&d).expect("clean");
    }

    #[test]
    fn clock_and_commas() {
        assert_eq!(clock(466_498), "6:28");
        assert_eq!(clock(72_000), "1:00");
        assert_eq!(commas(466_498), "466,498");
        assert_eq!(commas(999), "999");
        assert_eq!(commas(1_000), "1,000");
    }

    fn snap(seed: &str, stage: &str, ticks: i64, kind: Option<&str>) -> SnapTime {
        SnapTime {
            seed: seed.to_owned(),
            stage: stage.to_owned(),
            ticks,
            commit: "c".to_owned(),
            run: "1".to_owned(),
            lane: 0,
            bot: "b".to_owned(),
            kind: kind.map(str::to_owned),
        }
    }

    #[test]
    fn a_seed_folds_exact_stages_and_bounds_the_gaps() {
        let rows = fold(&[
            snap("typecraft", "log", 900, None),
            snap("typecraft", "log", 700, None),
            snap("typecraft", "nether", 50_000, None),
            snap("typecraft", "nether", 60_000, Some("stuck")),
            snap("desert", "table", 2_000, None),
        ]);
        assert_eq!(rows.len(), 2);
        let tc = rows.iter().find(|r| r.seed == "typecraft").expect("tc");
        assert_eq!(tc.furthest(), Some(Stage::Nether));
        assert_eq!(tc.stages.get(&Stage::Log), Some(&(700, Source::Exact)));
        // No table snapshot: bounded by the nether's.
        assert_eq!(
            tc.stages.get(&Stage::Table),
            Some(&(50_000, Source::AtMost))
        );
        assert_eq!(
            tc.stages.get(&Stage::Nether),
            Some(&(50_000, Source::Exact))
        );
        assert!(!tc.stages.contains_key(&Stage::Fortress));
        let md = render(&rows);
        assert!(md.contains("| typecraft | nether |"), "{md}");
        assert!(md.contains("| desert | table |"), "{md}");
        assert!(md.contains("700 (0:00)"), "{md}");
        assert!(md.contains("≤50,000 (0:41)"), "{md}");
    }

    #[test]
    fn snapshot_dirs_read_their_seed_and_leave_out_dev() {
        let d = tmp("snaps");
        let write = |name: &str, head: &str, time: i64| {
            let s = d.join(name);
            std::fs::create_dir_all(s.join("world")).expect("dir");
            let snap = crate::snapshot::Snap {
                stage: "log".to_owned(),
                bot: "rs-l00-b00".to_owned(),
                uuid: "u".to_owned(),
                server_ticks: 10,
                commit: "c".to_owned(),
                run: "9".to_owned(),
                lane: 0,
                pos: [0.0; 3],
                dimension: "minecraft:overworld".to_owned(),
                items: [("oak_log".to_owned(), 1)].into_iter().collect(),
            };
            let text = format!("{head}{}", toml::to_string(&snap).expect("toml"));
            std::fs::write(s.join("snap.toml"), text).expect("snap");
            std::fs::write(
                s.join("world").join("level.dat"),
                gz(&named(&level_nbt(time, 1))),
            )
            .expect("level");
        };
        write("a/log", "", 100);
        write("b/log", "seed = \"desert\"\n", 200);
        write("c/log", "dev = true\n", 300);
        let dirs = snapshot_dirs(&[d.to_string_lossy().into_owned()]);
        assert_eq!(dirs.len(), 3);
        let snaps: Vec<SnapTime> = dirs
            .iter()
            .filter_map(|p| read_snapshot(p).expect("read"))
            .collect();
        assert_eq!(snaps.len(), 2);
        assert_eq!(snaps[0].seed, DEFAULT_SEED);
        assert_eq!(snaps[0].ticks, 100);
        assert_eq!(snaps[1].seed, "desert");
        let back =
            toml::from_str::<SeedRows>(&rows_toml(&snaps, &[]).expect("rows")).expect("back");
        assert_eq!(back.rows, snaps);
        std::fs::remove_dir_all(&d).expect("clean");
    }

    #[test]
    fn a_lineage_parses_and_names_its_dirs() {
        let l = Lineage::parse(
            "seed = \"typecraft\"\n[[link]]\nstage = \"win\"\ntag = \"race-1\"\nasset = \"snapshot-win-1-race-1-lane-6.tar.zst\"\n",
        )
        .expect("parse");
        assert_eq!(l.links.len(), 1);
        assert_eq!(
            Lineage::link_dir(Path::new("/x"), &l.links[0]),
            PathBuf::from("/x/snapshot-win-1-race-1-lane-6")
        );
        assert!(
            Lineage::parse(
                "seed = \"t\"\n[[link]]\nstage = \"nope\"\ntag = \"a\"\nasset = \"b\"\n"
            )
            .is_err()
        );
    }

    /// The issue-20 win's lineage reads, climbs the ladder and ends at the frontier's asset.
    #[test]
    fn the_typecraft_lineage_climbs_to_the_win() {
        let l = Lineage::parse(include_str!("../lineage/typecraft.toml")).expect("parse");
        assert_eq!(l.seed, DEFAULT_SEED);
        let stages: Vec<Stage> = l
            .links
            .iter()
            .filter_map(|k| Stage::from_name(&k.stage))
            .collect();
        assert!(stages.windows(2).all(|w| w[0] <= w[1]), "{stages:?}");
        assert_eq!(
            l.links.last().map(|k| k.asset.as_str()),
            Some("snapshot-win-46108868213-race-46108868213-lane-6.tar.zst")
        );
    }

    fn row(bot: &str, ticks: &[(Stage, u64)], ending: crate::Ending) -> crate::Row {
        crate::Row {
            bot: bot.to_owned(),
            stage_reached: ticks.last().map(|t| t.0),
            ending,
            signature: Default::default(),
            server_ticks: 0,
            wall_ms: 0,
            case: None,
            recording: None,
            aim: None,
            pearls: None,
            eyes: None,
            end: None,
            stage_ticks: ticks.to_vec(),
            aim_tick: None,
        }
    }

    fn batch(lane: u32, source: crate::Source, rows: Vec<crate::Row>) -> crate::Batch {
        crate::Batch {
            format_version: crate::FORMAT_VERSION,
            run: "7".to_owned(),
            commit: "abcdef0123".to_owned(),
            lane,
            start: "spawn".to_owned(),
            source,
            config_hash: "0".to_owned(),
            bots: 1,
            server: "test".to_owned(),
            server_tick_rate: 20.0,
            rows,
            coverage: crate::Coverage::default(),
        }
    }

    /// Issue 69 on issue 56's stage ticks: per seed, the runs' ticks to each stage reached after
    /// the start (the start's own ladder left out), cut runs and sim batches not counted.
    #[test]
    fn run_ticks_group_by_seed_and_leave_out_the_start() {
        let d = tmp("lanes");
        let rel = d.join("race-7");
        std::fs::create_dir_all(rel.join("lane-1")).expect("dir");
        std::fs::write(
            rel.join("lanes.toml"),
            "[[lane]]\nlane = 0\nseed = \"typecraft\"\nstart = \"spawn\"\n[[lane]]\nlane = 1\nseed = \"desert\"\n",
        )
        .expect("lanes");
        assert_eq!(
            batch_seed(&rel.join("lane-1").join("batch.rsb"), 1),
            "desert"
        );
        assert_eq!(
            batch_seed(&rel.join("lane-1").join("batch.rsb"), 5),
            DEFAULT_SEED
        );
        assert_eq!(batch_seed(&d.join("x").join("batch.rsb"), 1), DEFAULT_SEED);
        let held = (Stage::InWorld, 20);
        let tc = batch(
            0,
            crate::Source::Real,
            vec![
                row("a", &[held, (Stage::Log, 300)], crate::Ending::Cap),
                row(
                    "b",
                    &[held, (Stage::Log, 500), (Stage::Table, 900)],
                    crate::Ending::Cap,
                ),
                row("c", &[held, (Stage::Log, 100)], crate::Ending::Cut),
            ],
        );
        let desert = batch(
            1,
            crate::Source::Real,
            vec![row("d", &[held], crate::Ending::Cap)],
        );
        let sim = batch(
            2,
            crate::Source::Sim,
            vec![row("e", &[held, (Stage::Log, 1)], crate::Ending::Cap)],
        );
        let all = [
            ("typecraft".to_owned(), rel.clone(), tc),
            ("desert".to_owned(), rel.clone(), desert),
            ("typecraft".to_owned(), rel.clone(), sim),
        ];
        let g = run_ticks(&all);
        let key = |s: &str| (s.to_owned(), "abcdef0123".to_owned(), "spawn".to_owned());
        let t = g.get(&key("typecraft")).expect("typecraft");
        assert_eq!(t.n, 2);
        assert_eq!(t.reached.get(&Stage::Log), Some(&vec![300, 500]));
        assert_eq!(t.reached.get(&Stage::Table), Some(&vec![900]));
        assert!(!t.reached.contains_key(&Stage::InWorld));
        assert_eq!(g.get(&key("desert")).map(|r| r.n), Some(1));
        let md = render_run_ticks(&g);
        assert!(
            md.contains("| typecraft | abcdef0 | spawn | log | 2/2 | 300 (0:00) | 300 | 500 |"),
            "{md}"
        );
        assert!(!md.contains("| desert |"), "{md}");
        // The stall: typecraft's furthest is table (b alone); desert's in_world. b's ledger.
        let ledger = crate::Ledger {
            format_version: crate::FORMAT_VERSION,
            run: "7".to_owned(),
            bot: "b".to_owned(),
            commit: "abcdef0123".to_owned(),
            tasks: vec![
                plan::TaskRecord::Started {
                    id: 1,
                    at: 10,
                    task: "walk".to_owned(),
                    what: Some("to the log".to_owned()),
                    why: Some("wood".to_owned()),
                },
                plan::TaskRecord::Ended {
                    id: 1,
                    at: 50,
                    task: "walk".to_owned(),
                    ticks: 40,
                    outcome: Some("walk failed: NoPath".to_owned()),
                    why: None,
                },
            ],
            dropped: 0,
            facts: Vec::new(),
        };
        crate::write(&rel.join("b.ledger.rsl"), &ledger).expect("ledger");
        let st = stalls(&all);
        assert_eq!(st.len(), 2);
        let t = st.iter().find(|s| s.seed == "typecraft").expect("tc stall");
        assert_eq!(
            (t.stage.as_str(), t.k, t.n, t.bot.as_str()),
            ("table", 1, 2, "b")
        );
        assert_eq!(t.reasons.len(), 1);
        assert_eq!(
            t.last_task,
            "walk: to the log (wood) -> walk failed: NoPath"
        );
        let back: SeedRows = toml::from_str(&rows_toml(&[], &st).expect("toml")).expect("back");
        assert_eq!(back.stall, st);
        assert!(
            render_stalls(&st).contains("stall `table` (1/2"),
            "{}",
            render_stalls(&st)
        );
        std::fs::remove_dir_all(&d).expect("clean");
    }
}
