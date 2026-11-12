//! Race results as files (brain/ruststeve/design/race-and-results.md): a case file for every
//! ended run and a batch file for every lane, MessagePack with named fields in zstd, like the
//! recordings (brain/ruststeve/decisions/recording-format.md), so the insight store and steve
//! read them by name. They grow by accretion: add fields (with `#[serde(default)]`) and
//! variants, never rename or repurpose one.

#![forbid(unsafe_code)]

pub mod config;
pub mod parity;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use world::{Hit, Stage};

/// The results formats' version; a reader refuses one it does not know.
pub const FORMAT_VERSION: u32 = 1;

/// zstd level, as the recordings use.
const ZSTD_LEVEL: i32 = 3;

/// Where a run's numbers came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Source {
    Real,
    Sim,
}

/// How a run ended. A closed set: every ended run has exactly one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ending {
    /// The server told of the bot's death: the killer in its own words.
    Death { killer: String },
    /// The start's cap in server ticks was reached.
    Cap,
    /// The server sent a disconnect.
    Kicked { reason: String },
    /// The connection closed without a word.
    Lost { why: String },
    /// The bot itself failed (a recording write, the writer thread).
    Error { why: String },
    /// The job's end stopped it: not counted in n (brain/ruststeve/design/race-and-results.md).
    Cut,
}

impl Ending {
    /// The short name signatures and the store group by.
    pub fn name(&self) -> &'static str {
        match self {
            Ending::Death { .. } => "death",
            Ending::Cap => "cap",
            Ending::Kicked { .. } => "kicked",
            Ending::Lost { .. } => "lost",
            Ending::Error { .. } => "error",
            Ending::Cut => "cut",
        }
    }

    /// Whether the run counts in n.
    pub fn counted(&self) -> bool {
        !matches!(self, Ending::Cut)
    }
}

/// What the bot was doing when the run ended, and why it chose that.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Doing {
    pub task: String,
    pub why: String,
}

/// The case file: one per ended run (clojurecraft's `.case.txt`, as data).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Case {
    pub format_version: u32,
    /// The race's run id (the workflow run, or `local-<ms>`) and the bot's name: together the
    /// stable id the store and the live stream share.
    pub run: String,
    pub bot: String,
    pub commit: String,
    pub lane: u32,
    pub start: String,
    pub source: Source,
    pub stage_reached: Option<Stage>,
    pub ending: Ending,
    /// The server's death message, whole, when there was one.
    pub death_text: Option<String>,
    /// The first hit in the last minute before the end, and every hit in that minute.
    pub first_hit: Option<Hit>,
    pub hits: Vec<Hit>,
    pub doing: Doing,
    /// Items by name: counts gained and lost between the start (after its kit) and the end.
    pub items_gained: BTreeMap<String, i64>,
    pub items_lost: BTreeMap<String, i64>,
    pub swings_sent: u64,
    pub swings_landed: u64,
    pub health: Option<f32>,
    pub pos: Option<[f64; 3]>,
    pub dimension: Option<String>,
    pub server_ticks: u64,
    pub wall_ms: u64,
    /// The recording, relative to the batch directory, and how to replay it.
    pub recording: Option<String>,
    pub replay: String,
    /// What the live stream sent, dropped and failed, when the run streamed (issue 07c).
    #[serde(default)]
    pub stream: Option<StreamCounts>,
    /// The planner's account, when the run had an aim (issue 09).
    #[serde(default)]
    pub plan: Option<PlanSummary>,
}

/// What the planner did in a run: its aim, whether it was met or the planner stopped stuck,
/// walks started and why they failed, the longest any walk went without getting closer (the
/// progress invariant's measure) and what its searches spent.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanSummary {
    pub aim: String,
    pub reached: bool,
    pub stuck: bool,
    pub walks: u64,
    pub failures: BTreeMap<String, u64>,
    pub longest_stall: u64,
    pub expanded: u64,
    /// The brain's time per tick in the shell's measure, microseconds: mean, p99 and max.
    #[serde(default)]
    pub tick_us: Option<[u64; 3]>,
}

/// The live stream's counts for one run (schema/snapshot-v1.md).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamCounts {
    pub sent: u64,
    pub dropped: u64,
    pub failed: u64,
    pub connects: u64,
}

impl Case {
    /// `[ending killer doing why]`: what the batch groups endings by.
    pub fn signature(&self) -> [String; 4] {
        let killer = match &self.ending {
            Ending::Death { killer } => killer.clone(),
            Ending::Kicked { reason } => reason.clone(),
            Ending::Lost { why } | Ending::Error { why } => why.clone(),
            Ending::Cap | Ending::Cut => String::new(),
        };
        [
            self.ending.name().to_owned(),
            killer,
            self.doing.task.clone(),
            self.doing.why.clone(),
        ]
    }

    /// The case as text, for reading in an artifact.
    pub fn render(&self) -> String {
        let hit = |h: &Hit| {
            format!(
                "tick {} {} by {} via {}",
                h.at_tick,
                h.damage_type,
                h.cause.map_or("-".to_owned(), |k| format!("{k:?}")),
                h.direct.map_or("-".to_owned(), |k| format!("{k:?}"))
            )
        };
        let items = |m: &BTreeMap<String, i64>| {
            if m.is_empty() {
                "-".to_owned()
            } else {
                m.iter()
                    .map(|(k, v)| format!("{k} {v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        };
        let mut lines = vec![
            format!("case {} / {}", self.run, self.bot),
            format!(
                "commit {}  lane {}  start {}  source {:?}",
                self.commit, self.lane, self.start, self.source
            ),
            format!(
                "stage reached: {}",
                self.stage_reached.map_or("none", Stage::name)
            ),
            format!("ending: {:?}", self.ending),
            format!(
                "death message: {}",
                self.death_text.as_deref().unwrap_or("-")
            ),
            format!(
                "first hit in the last minute: {}",
                self.first_hit.as_ref().map_or("-".to_owned(), hit)
            ),
            format!("hits in the last minute: {}", self.hits.len()),
            format!("doing: {} (why: {})", self.doing.task, self.doing.why),
            format!("items gained: {}", items(&self.items_gained)),
            format!("items lost: {}", items(&self.items_lost)),
            format!(
                "swings sent {}, landed {}",
                self.swings_sent, self.swings_landed
            ),
            format!(
                "health {:?}  at {:?} in {}",
                self.health,
                self.pos,
                self.dimension.as_deref().unwrap_or("-")
            ),
            format!(
                "{} server ticks, {:.1} s",
                self.server_ticks,
                self.wall_ms as f64 / 1000.0
            ),
            format!("replay: {}", self.replay),
        ];
        if let Some(p) = &self.plan {
            lines.push(format!(
                "plan {}: reached {}, stuck {}, {} walks, failures {:?}, longest stall {} ticks, {} expansions, tick us {:?}",
                p.aim, p.reached, p.stuck, p.walks, p.failures, p.longest_stall, p.expanded, p.tick_us
            ));
        }
        if let Some(s) = self.stream {
            lines.push(format!(
                "stream: sent {}, dropped {}, failed {}, connects {}",
                s.sent, s.dropped, s.failed, s.connects
            ));
        }
        lines.extend(self.hits.iter().map(|h| format!("  hit {}", hit(h))));
        lines.join("\n") + "\n"
    }
}

/// One run's row in a batch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub bot: String,
    pub stage_reached: Option<Stage>,
    pub ending: Ending,
    pub signature: [String; 4],
    pub server_ticks: u64,
    pub wall_ms: u64,
    /// The case file and recording, relative to the batch directory.
    pub case: Option<String>,
    pub recording: Option<String>,
    /// With an aim (issue 09): the aim and whether the planner met it.
    #[serde(default)]
    pub aim: Option<(String, bool)>,
}

/// What a batch could see: full sight is every ratio 1.0 and the counts 0.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    pub runs: u64,
    pub cases: u64,
    pub recordings: u64,
    /// Events the bot dropped before stepping them (decode errors): never folded, never seen.
    pub events_dropped: u64,
    /// Packets with an id the protocol layer does not model, summed over runs.
    pub unknown_packets: u64,
}

impl Coverage {
    pub fn cases_ratio(&self) -> f64 {
        ratio(self.cases, self.runs)
    }

    pub fn recordings_ratio(&self) -> f64 {
        ratio(self.recordings, self.runs)
    }
}

fn ratio(k: u64, n: u64) -> f64 {
    if n == 0 { 1.0 } else { k as f64 / n as f64 }
}

/// A lane's batch file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Batch {
    pub format_version: u32,
    pub run: String,
    pub commit: String,
    pub lane: u32,
    pub start: String,
    pub source: Source,
    /// FNV-1a of `config/race.toml` as run.
    pub config_hash: String,
    pub bots: u32,
    pub server: String,
    /// Server ticks per wall second, the busiest bot's view: how hard the server worked.
    pub server_tick_rate: f64,
    pub rows: Vec<Row>,
    pub coverage: Coverage,
}

/// What a file holds, so one reader opens either.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ResultFile {
    Case(Box<Case>),
    Batch(Box<Batch>),
}

pub fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = rmp_serde::to_vec_named(value).map_err(|e| format!("encode: {e}"))?;
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut z = zstd::Encoder::new(file, ZSTD_LEVEL).map_err(|e| e.to_string())?;
    z.write_all(&bytes).map_err(|e| e.to_string())?;
    z.finish().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    zstd::Decoder::new(file)
        .map_err(|e| e.to_string())?
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    rmp_serde::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// A batch file, refusing a format version this build does not know.
pub fn read_batch(path: &Path) -> Result<Batch, String> {
    let b: Batch = read(path)?;
    if b.format_version != FORMAT_VERSION {
        return Err(format!(
            "{}: batch format {} (this build reads {FORMAT_VERSION})",
            path.display(),
            b.format_version
        ));
    }
    Ok(b)
}

pub fn read_case(path: &Path) -> Result<Case, String> {
    let c: Case = read(path)?;
    if c.format_version != FORMAT_VERSION {
        return Err(format!(
            "{}: case format {} (this build reads {FORMAT_VERSION})",
            path.display(),
            c.format_version
        ));
    }
    Ok(c)
}

/// k/n for one stage in one (commit, start, source) group: k runs reached it, of n counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Kn {
    pub k: u64,
    pub n: u64,
}

/// `(commit, start, source) -> stage -> k/n`, over every counted run of every batch. k counts
/// runs whose stage reached is at least the stage.
pub type Summary = BTreeMap<(String, String, Source), BTreeMap<Stage, Kn>>;

pub fn summarize(batches: &[Batch]) -> Summary {
    let mut out: Summary = BTreeMap::new();
    for b in batches {
        let group = out
            .entry((b.commit.clone(), b.start.clone(), b.source))
            .or_default();
        for row in b.rows.iter().filter(|r| r.ending.counted()) {
            for stage in Stage::ALL {
                let kn = group.entry(stage).or_default();
                kn.n += 1;
                if row.stage_reached.is_some_and(|s| s >= stage) {
                    kn.k += 1;
                }
            }
        }
    }
    out
}

/// The summary as a markdown table, for a job summary.
pub fn render_summary(s: &Summary, batches: &[Batch]) -> String {
    let mut lines = vec![
        "| commit | start | source | stage | k/n | rate |".to_owned(),
        "|---|---|---|---|---|---|".to_owned(),
    ];
    for ((commit, start, source), stages) in s {
        let short = commit.get(..7).unwrap_or(commit);
        for (stage, kn) in stages {
            lines.push(format!(
                "| {short} | {start} | {source:?} | {} | {}/{} | {:.1}% |",
                stage.name(),
                kn.k,
                kn.n,
                100.0 * ratio(kn.k, kn.n)
            ));
        }
    }
    let total = batches.iter().fold(Coverage::default(), |mut c, b| {
        c.runs += b.coverage.runs;
        c.cases += b.coverage.cases;
        c.recordings += b.coverage.recordings;
        c.events_dropped += b.coverage.events_dropped;
        c.unknown_packets += b.coverage.unknown_packets;
        c
    });
    lines.push(String::new());
    lines.push(format!(
        "coverage: cases {}/{} ({:.2}), recordings {}/{} ({:.2}), events dropped {}, unknown packets {}",
        total.cases,
        total.runs,
        total.cases_ratio(),
        total.recordings,
        total.runs,
        total.recordings_ratio(),
        total.events_dropped,
        total.unknown_packets
    ));
    // The planner's aims, when runs had one: met over counted runs, per aim.
    let mut aims: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for row in batches
        .iter()
        .flat_map(|b| &b.rows)
        .filter(|r| r.ending.counted())
    {
        if let Some((aim, met)) = &row.aim {
            let e = aims.entry(aim.clone()).or_default();
            e.1 += 1;
            e.0 += u64::from(*met);
        }
    }
    for (aim, (k, n)) in &aims {
        lines.push(format!(
            "aim {aim}: met {k}/{n} ({:.1}%)",
            100.0 * ratio(*k, *n)
        ));
    }
    let mut endings: BTreeMap<[String; 4], u64> = BTreeMap::new();
    for row in batches.iter().flat_map(|b| &b.rows) {
        *endings.entry(row.signature.clone()).or_default() += 1;
    }
    let mut ranked: Vec<_> = endings.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    lines.push(String::new());
    lines.push("| runs | ending | killer | doing | why |".to_owned());
    lines.push("|---|---|---|---|---|".to_owned());
    for ([ending, killer, doing, why], n) in ranked {
        lines.push(format!("| {n} | {ending} | {killer} | {doing} | {why} |"));
    }
    lines.join("\n") + "\n"
}
