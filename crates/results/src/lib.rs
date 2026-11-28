//! Race results as files (brain/ruststeve/design/race-and-results.md): a case file for every
//! ended run and a batch file for every lane, MessagePack with named fields in zstd, like the
//! recordings (brain/ruststeve/decisions/recording-format.md), so the insight store and steve
//! read them by name. They grow by accretion: add fields (with `#[serde(default)]`) and
//! variants, never rename or repurpose one.

#![forbid(unsafe_code)]

pub mod census;
pub mod config;
pub mod diagnose;
pub mod keep;
pub mod observe;
pub mod parity;
pub mod parity_notes;
pub mod region;
pub mod seeds;
pub mod snapshot;
pub mod story;
pub mod text;
pub mod truth;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use world::{Hit, Stage};

/// The planner's ledger record, for readers that build a `Ledger` without the planner (the
/// `ledger` binary re-reading a recording).
pub use plan::TaskRecord;

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
    /// No progress (no stage, no item gained, no new place) for `for_ticks`: the bot was ended
    /// with its case rather than left to the cap (issue 16e). Counted: a stuck run is a result.
    NoProgress { for_ticks: u64, task: String },
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
            Ending::NoProgress { .. } => "no-progress",
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
    /// The run's story from its ledger and senses (issue 16e): when present the case prints it
    /// in place of the cut notes.
    #[serde(default)]
    pub story: Option<story::Story>,
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
    /// The planner's notes in order ("tick what (why)"), runs of two alternating kinds
    /// collapsed, the middle dropped past `bot::NOTES_KEPT`: what the bot decided and why, read
    /// from the case file with no replay (a race was read by exporting every recording).
    #[serde(default)]
    pub notes: Vec<String>,
    /// Notes left out of `notes` (collapsed repeats and the dropped middle).
    #[serde(default)]
    pub notes_dropped: u64,
    /// The server tick the aim was first met (issue 56); `None` when it never was, or in older
    /// cases.
    #[serde(default)]
    pub met_at: Option<u64>,
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
    /// Ender pearls gained between the start and the end, net (issue 18b): from a start with
    /// none, the pearls held at the end.
    pub fn pearls_gained(&self) -> i64 {
        let n = |m: &BTreeMap<String, i64>| m.get("ender_pearl").copied().unwrap_or(0);
        n(&self.items_gained) - n(&self.items_lost)
    }

    /// The run's eyes of ender (issue 19b), from its story.
    pub fn eyes(&self) -> Option<story::EyeCounts> {
        self.story.as_ref().and_then(|s| s.eyes)
    }

    /// The run's End (issue 20b), from its story.
    pub fn end(&self) -> Option<story::EndCounts> {
        self.story.as_ref().and_then(|s| s.end.clone())
    }

    /// The tick each stage was first reached (issue 56), from its story; empty without one.
    pub fn stage_ticks(&self) -> Vec<(Stage, u64)> {
        self.story
            .as_ref()
            .map(|s| s.stage_ticks.clone())
            .unwrap_or_default()
    }

    /// The tick the aim was first met (issue 56), from the planner's account.
    pub fn aim_tick(&self) -> Option<u64> {
        self.plan.as_ref().and_then(|p| p.met_at)
    }

    /// `[ending killer doing why]`: what the batch groups endings by.
    pub fn signature(&self) -> [String; 4] {
        let killer = match &self.ending {
            Ending::Death { killer } => killer.clone(),
            Ending::Kicked { reason } => reason.clone(),
            Ending::Lost { why } | Ending::Error { why } => why.clone(),
            Ending::NoProgress { task, .. } => task.clone(),
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
        ];
        // Where the bot stood at the end, in plain terms, before anything else is read.
        if let Some(s) = &self.story
            && !s.standing.is_empty()
        {
            lines.push("where:".to_owned());
            lines.extend(s.standing.iter().map(|l| format!("  {l}")));
        }
        lines.extend([
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
            // The feet's cell is the position floored, as the planner has it: a position
            // rounded by eye reads one cell off (-18.3 is cell -19).
            format!(
                "health {}  at {} in {}",
                self.health
                    .map_or_else(|| "-".to_owned(), |h| format!("{h:.1}")),
                self.pos.map_or_else(
                    || "-".to_owned(),
                    |p| {
                        let [x, y, z] = ::observe::feet_cell(p);
                        format!(
                            "{:.2},{:.2},{:.2} (feet cell {x},{y},{z})",
                            p[0], p[1], p[2]
                        )
                    }
                ),
                self.dimension.as_deref().unwrap_or("-")
            ),
            format!(
                "{} server ticks, {:.1} s",
                self.server_ticks,
                self.wall_ms as f64 / 1000.0
            ),
            format!("replay: {}", self.replay),
        ]);
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
        if let Some(s) = &self.story {
            lines.extend(s.render());
        } else if let Some(p) = &self.plan {
            if p.notes_dropped > 0 {
                lines.push(format!("notes left out: {}", p.notes_dropped));
            }
            lines.extend(p.notes.iter().map(|n| format!("  note {n}")));
        }
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
    /// Ender pearls gained over the run (`Case::pearls_gained`, issue 18b); `None` in rows from
    /// before it was kept.
    #[serde(default)]
    pub pearls: Option<i64>,
    /// The run's eyes (`Case::eyes`, issue 19b); `None` when it had none or in older rows.
    #[serde(default)]
    pub eyes: Option<story::EyeCounts>,
    /// The run's End (`Case::end`, issue 20b); `None` when it had none or in older rows.
    #[serde(default)]
    pub end: Option<story::EndCounts>,
    /// The tick each stage was first reached (`Case::stage_ticks`, issue 56), stage order; empty
    /// in older rows and runs with no ledger.
    #[serde(default)]
    pub stage_ticks: Vec<(Stage, u64)>,
    /// The tick the aim was first met (`Case::aim_tick`, issue 56); `None` when not met or in
    /// older rows.
    #[serde(default)]
    pub aim_tick: Option<u64>,
}

/// What a batch could see: full sight is every ratio 1.0 and the counts 0 (issue 16e). A column
/// that is `None` is not measured yet, which the summary marks as blind: it never reads as fine.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    pub runs: u64,
    pub cases: u64,
    pub recordings: u64,
    /// Events the bot dropped before stepping them (decode errors): never folded, never seen.
    pub events_dropped: u64,
    /// Packets with an id the protocol layer does not model, summed over runs.
    pub unknown_packets: u64,
    /// The same, by `state/name` (`protocol::packet_name`).
    #[serde(default)]
    pub unknown_by_kind: BTreeMap<String, u64>,
    /// Inventory slots whose item components could not all be read (the rest of the frame lost).
    #[serde(default)]
    pub slots_truncated: u64,
    /// The same, by the item component that stopped the read: the next reader to add.
    #[serde(default)]
    pub slots_truncated_by: BTreeMap<String, u64>,
    /// Entity data cut short at a serializer the bot cannot read.
    #[serde(default)]
    pub metadata_truncated: Option<u64>,
    /// The same, by the serializer the read stopped at.
    #[serde(default)]
    pub metadata_truncated_by: BTreeMap<String, u64>,
    /// Runs with each of the files a run should leave (issue 16e phases C and D).
    #[serde(default)]
    pub ledgers: Option<u64>,
    #[serde(default)]
    pub senses: Option<u64>,
    #[serde(default)]
    pub moments: Option<u64>,
    #[serde(default)]
    pub truths: Option<u64>,
    /// Tasks the planner started, those that carried a reason, and those that ended.
    #[serde(default)]
    pub tasks_started: Option<u64>,
    #[serde(default)]
    pub tasks_with_reason: Option<u64>,
    #[serde(default)]
    pub tasks_ended: Option<u64>,
    /// Ledger events the bot could not write (must be 0).
    #[serde(default)]
    pub ledger_dropped: Option<u64>,
}

/// Entity-data serializers left unread on purpose, with why: a stop at one is counted apart and
/// is no blind spot (issue 16e).
pub const SKIPPED_SERIALIZERS: [(&str, &str); 4] = [
    (
        "PARTICLES",
        "a living entity's effect particles, each type laid out its own way; after them only arrow and stinger counts and a sleeping place",
    ),
    (
        "PARTICLE",
        "an area cloud's particle, its last entry; the cloud's radius comes before it",
    ),
    (
        "RESOLVABLE_PROFILE",
        "a mannequin's or a head's player profile",
    ),
    (
        "PAINTING_VARIANT",
        "a painting's variant, which may be written inline",
    ),
];

/// Adds an optional count: unmeasured anywhere stays unmeasured.
fn add_opt(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    Some(a? + b?)
}

impl Coverage {
    /// Two batches' coverage, summed.
    pub fn add(&mut self, b: &Coverage) {
        self.runs += b.runs;
        self.cases += b.cases;
        self.recordings += b.recordings;
        self.events_dropped += b.events_dropped;
        self.unknown_packets += b.unknown_packets;
        for (k, n) in &b.unknown_by_kind {
            *self.unknown_by_kind.entry(k.clone()).or_default() += n;
        }
        self.slots_truncated += b.slots_truncated;
        for (k, n) in &b.slots_truncated_by {
            *self.slots_truncated_by.entry(k.clone()).or_default() += n;
        }
        self.metadata_truncated = add_opt(self.metadata_truncated, b.metadata_truncated);
        for (k, n) in &b.metadata_truncated_by {
            *self.metadata_truncated_by.entry(k.clone()).or_default() += n;
        }
        self.ledgers = add_opt(self.ledgers, b.ledgers);
        self.senses = add_opt(self.senses, b.senses);
        self.moments = add_opt(self.moments, b.moments);
        self.truths = add_opt(self.truths, b.truths);
        self.tasks_started = add_opt(self.tasks_started, b.tasks_started);
        self.tasks_with_reason = add_opt(self.tasks_with_reason, b.tasks_with_reason);
        self.tasks_ended = add_opt(self.tasks_ended, b.tasks_ended);
        self.ledger_dropped = add_opt(self.ledger_dropped, b.ledger_dropped);
    }

    /// The coverage block of a summary: one line a column, `!!` on each blind spot, then the
    /// packets thrown away by kind, wanted (blind) before skipped.
    pub fn render(&self, observe: Option<&observe::Observe>) -> Vec<String> {
        const BLIND: &str = " !!";
        let runs = self.runs;
        let have = |name: &str, n: u64, want: u64| {
            let mark = if n < want { BLIND } else { "" };
            format!("- {name}: {n}/{want}{mark}")
        };
        let have_opt = |name: &str, n: Option<u64>, want: u64| match n {
            Some(n) => have(name, n, want),
            None => format!("- {name}: not measured{BLIND}"),
        };
        let zero = |name: &str, n: u64| {
            let mark = if n > 0 { BLIND } else { "" };
            format!("- {name}: {n}{mark}")
        };
        let zero_opt = |name: &str, n: Option<u64>| match n {
            Some(n) => zero(name, n),
            None => format!("- {name}: not measured{BLIND}"),
        };
        let mut lines = vec![
            "### coverage (`!!` is a blind spot, fixed before any bot change)".to_owned(),
            String::new(),
            have("cases", self.cases, runs),
            have("recordings", self.recordings, runs),
            have_opt("ledgers", self.ledgers, runs),
            have_opt("senses lines", self.senses, runs),
            have_opt("moments", self.moments, runs),
            have_opt("server truth", self.truths, runs),
        ];
        match self.tasks_started {
            Some(started) => {
                lines.push(have_opt(
                    "tasks with a reason",
                    self.tasks_with_reason,
                    started,
                ));
                lines.push(have_opt("tasks ended", self.tasks_ended, started));
            }
            None => {
                lines.push(format!("- tasks with a reason: not measured{BLIND}"));
                lines.push(format!("- tasks ended: not measured{BLIND}"));
            }
        }
        lines.push(zero_opt("ledger events dropped", self.ledger_dropped));
        lines.push(zero("decode errors", self.events_dropped));
        lines.push(zero("item slots cut short", self.slots_truncated));
        for (component, n) in &self.slots_truncated_by {
            lines.push(format!("  - at component `{component}`: {n}"));
        }
        // Stops at a serializer left unread on purpose are counted apart, with the reason.
        let on_purpose = |s: &str| SKIPPED_SERIALIZERS.iter().find(|(n, _)| *n == s);
        let blind_stops: u64 = self
            .metadata_truncated_by
            .iter()
            .filter(|(s, _)| on_purpose(s).is_none())
            .map(|(_, n)| n)
            .sum();
        lines.push(zero_opt(
            "entity data cut short",
            self.metadata_truncated.map(|_| blind_stops),
        ));
        for (serializer, n) in &self.metadata_truncated_by {
            match on_purpose(serializer) {
                Some((_, why)) => {
                    lines.push(format!("  - at `{serializer}`, on purpose ({why}): {n}"));
                }
                None => lines.push(format!("  - at serializer `{serializer}`: {n}")),
            }
        }
        // Packets thrown away, by class then count.
        let class = |k: &str| observe.map_or(observe::Class::Unclassed, |o| o.class(k));
        let mut kinds: Vec<(observe::Class, &String, u64)> = self
            .unknown_by_kind
            .iter()
            .map(|(k, n)| (class(k), k, *n))
            .collect();
        kinds.sort_by(|a, b| a.0.cmp(&b.0).then(b.2.cmp(&a.2)).then(a.1.cmp(b.1)));
        let blind: u64 = kinds.iter().filter(|k| k.0.blind()).map(|k| k.2).sum();
        let skipped: u64 = kinds.iter().filter(|k| !k.0.blind()).map(|k| k.2).sum();
        let per_run = |n: u64| n as f64 / runs.max(1) as f64;
        lines.push(zero("packets thrown away that the stages need", blind));
        lines.push(format!(
            "- packets thrown away on purpose: {skipped} ({})",
            observe::PATH
        ));
        if self.unknown_by_kind.is_empty() && self.unknown_packets > 0 {
            lines.push(format!(
                "- unknown packets not broken down by kind (an older batch): {}{BLIND}",
                self.unknown_packets
            ));
        }
        if !kinds.is_empty() {
            lines.push(String::new());
            lines.push("| packet thrown away | class | total | per run |".to_owned());
            lines.push("|---|---|---|---|".to_owned());
            for (c, k, n) in &kinds {
                let label = match c {
                    observe::Class::Wanted => "wanted !!",
                    observe::Class::Unclassed => "unclassed !!",
                    observe::Class::Skipped => "skipped",
                };
                lines.push(format!("| {k} | {label} | {n} | {:.1} |", per_run(*n)));
            }
        }
        lines
    }
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

/// A run's ledger (issue 16e): every task the planner started and ended, unsampled, with its
/// reason and outcome. Written beside the case as `<bot>.ledger.rsl`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    pub format_version: u32,
    pub run: String,
    pub bot: String,
    pub commit: String,
    pub tasks: Vec<plan::TaskRecord>,
    /// Records the bot could not keep (must be 0).
    #[serde(default)]
    pub dropped: u64,
    /// What the bot sensed, in order: health, hits, items and tools, sounds, mobs and their
    /// signs (`observe::diff`).
    #[serde(default)]
    pub facts: Vec<::observe::Stamped>,
}

impl SensesFile {
    /// One line every `every` rows (1 is every second): the tick, the position to one decimal,
    /// the feet's cell as the planner has it (floored: a rounded position reads one cell off),
    /// what the feet stand on and any lava or water against the body, then health, food, the
    /// held item, the task with its reason, and the nearest hostiles. A change of dimension is
    /// a line of its own.
    pub fn render(&self, every: usize) -> String {
        let mut out = format!(
            "senses {} / {}: {} rows (x,y,z to one decimal; cell is the feet's cell, floored, as the planner has it)\n",
            self.run,
            self.bot,
            self.rows.len()
        );
        let mut dimension: Option<&str> = None;
        for r in self.rows.iter().step_by(every.max(1)) {
            if r.dimension.is_some() && r.dimension.as_deref() != dimension {
                dimension = r.dimension.as_deref();
                out.push_str(&format!("{:>8} in {}\n", r.at, dimension.unwrap_or("-")));
            }
            let pos = r.pos.map_or_else(
                || "-".to_owned(),
                |p| format!("{:.1},{:.1},{:.1}", p[0], p[1], p[2]),
            );
            let near: Vec<String> = r
                .hostiles
                .iter()
                .take(3)
                .map(|h| {
                    let mut s = format!("{} at {:.0}", h.kind.name().unwrap_or("?"), h.d);
                    for (on, sign) in [
                        (h.signs.fuse, "fuse"),
                        (h.signs.charge, "charge"),
                        (h.signs.angry, "angry"),
                        (h.signs.facing, "facing"),
                    ] {
                        if on {
                            s.push_str(&format!(" +{sign}"));
                        }
                    }
                    s
                })
                .collect();
            out.push_str(&format!(
                "{:>8} {pos} {} hp {} food {} held {}{}{}{}{}{}{}{} | {} ({}) | {}\n",
                r.at,
                r.standing(),
                r.health
                    .map_or_else(|| "-".to_owned(), |h| format!("{h:.1}")),
                r.food.map_or_else(|| "-".to_owned(), |f| f.to_string()),
                r.held.as_deref().unwrap_or("nothing"),
                r.held_uses
                    .map_or_else(String::new, |u| format!(" ({u} uses)")),
                r.cursor
                    .as_ref()
                    .map_or_else(String::new, |(i, n)| format!(" cursor {i} {n}")),
                r.day_time.map_or_else(String::new, |t| format!(
                    " day {t}{}",
                    if world::time::is_night(t) {
                        " night"
                    } else {
                        ""
                    }
                )),
                r.travel(),
                r.hunt(),
                r.eyes(),
                r.end(),
                r.task,
                r.why,
                near.join(", ")
            ));
        }
        out
    }
}

impl Ledger {
    /// Every task start and end, every note and every fact, by tick (`text`): the planner's
    /// records in its own order, a run of the same records once with its count, one second's
    /// changes to an item on one line. The header counts what the file holds, not the lines.
    pub fn lines(&self) -> Vec<(u64, String)> {
        text::merged(&self.tasks, &self.facts)
    }

    pub fn render(&self) -> String {
        let lines = self.lines();
        let mut out = format!(
            "ledger {} / {}: {} task records, {} facts, {} dropped, told in {} lines (a run of the same records is one line: [xN, to tick T])\n",
            self.run,
            self.bot,
            self.tasks.len(),
            self.facts.len(),
            self.dropped,
            lines.len()
        );
        for (at, l) in lines {
            out.push_str(&format!("{at:>8} {l}\n"));
        }
        out
    }
}

/// A run's senses: one row a second (`<bot>.senses.rss`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SensesFile {
    pub format_version: u32,
    pub run: String,
    pub bot: String,
    pub rows: Vec<::observe::Senses>,
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

/// Fills the stage ticks of rows that have none (batches from before issue 56) from each bot's
/// ledger beside the batch, `<dir>/<bot>.ledger.rsl`, so an old race gets its per-stage ticks
/// too. A row with no ledger there stays as it was. Returns the rows filled.
pub fn fill_stage_ticks(batch: &mut Batch, dir: &Path) -> usize {
    let mut filled = 0;
    for row in batch.rows.iter_mut().filter(|r| r.stage_ticks.is_empty()) {
        let path = dir.join(format!("{}.ledger.rsl", row.bot));
        if !path.is_file() {
            continue;
        }
        if let Ok(l) = read::<Ledger>(&path) {
            row.stage_ticks = story::stage_ticks(&l);
            filled += usize::from(!row.stage_ticks.is_empty());
        }
    }
    filled
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

/// The summary as markdown, for a job summary: coverage first (a blind spot is read before any
/// rate), then k/n per stage, the aims and the endings ranked.
pub fn render_summary(
    s: &Summary,
    batches: &[Batch],
    observe: Option<&observe::Observe>,
) -> String {
    // From the first batch: an unmeasured column stays unmeasured only if some batch lacks it.
    let mut total = batches
        .first()
        .map(|b| b.coverage.clone())
        .unwrap_or_default();
    for b in batches.iter().skip(1) {
        total.add(&b.coverage);
    }
    let mut lines = total.render(observe);
    lines.push(String::new());
    lines.push("| commit | start | source | stage | k/n | rate |".to_owned());
    lines.push("|---|---|---|---|---|---|".to_owned());
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
    let ticks = stage_tick_lines(batches);
    if !ticks.is_empty() {
        lines.push(String::new());
        lines.extend(ticks);
    }
    if let Some(line) = pearls_line(batches) {
        lines.push(String::new());
        lines.push(line);
    }
    let eyes = eyes_lines(batches);
    if !eyes.is_empty() {
        lines.push(String::new());
        lines.extend(eyes);
    }
    let end = end_lines(batches);
    if !end.is_empty() {
        lines.push(String::new());
        lines.extend(end);
    }
    // The planner's aims, when runs had one: met over counted runs, per aim, and the tick it was
    // met at over the runs that kept it (issue 56).
    let mut aims: BTreeMap<String, (u64, u64, Vec<u64>)> = BTreeMap::new();
    for row in batches
        .iter()
        .flat_map(|b| &b.rows)
        .filter(|r| r.ending.counted())
    {
        if let Some((aim, met)) = &row.aim {
            let e = aims.entry(aim.clone()).or_default();
            e.1 += 1;
            e.0 += u64::from(*met);
            e.2.extend(row.aim_tick.filter(|_| *met));
        }
    }
    for (aim, (k, n, at)) in &mut aims {
        at.sort_unstable();
        let when = match quartiles(at) {
            Some([p25, median, p75]) => format!(
                "; met at median tick {median} (p25 {p25}, p75 {p75}, {} runs)",
                at.len()
            ),
            None => String::new(),
        };
        lines.push(format!(
            "aim {aim}: met {k}/{n} ({:.1}%){when}",
            100.0 * ratio(*k, *n)
        ));
    }
    // Endings ranked, each with the first three bots to read (AGENTS.md section 4: three case
    // files of the top signature, issue 16e).
    let mut endings: BTreeMap<[String; 4], (u64, Vec<String>)> = BTreeMap::new();
    for (b, row) in batches
        .iter()
        .flat_map(|b| b.rows.iter().map(move |r| (b, r)))
    {
        let e = endings.entry(row.signature.clone()).or_default();
        e.0 += 1;
        if e.1.len() < 3 {
            e.1.push(format!("lane {} {}", b.lane, row.bot));
        }
    }
    let mut ranked: Vec<_> = endings.into_iter().collect();
    ranked.sort_by(|a, b| b.1.0.cmp(&a.1.0).then_with(|| a.0.cmp(&b.0)));
    lines.push(String::new());
    lines.push("| runs | ending | killer | doing | why | read first |".to_owned());
    lines.push("|---|---|---|---|---|---|".to_owned());
    for ([ending, killer, doing, why], (n, bots)) in ranked {
        lines.push(format!(
            "| {n} | {ending} | {killer} | {doing} | {why} | {} |",
            bots.join(", ")
        ));
    }
    lines.join("\n") + "\n"
}

/// p25, median and p75 of sorted ticks, nearest rank (the smallest value with at least that share
/// at or below it); `None` when there are none.
pub fn quartiles(sorted: &[u64]) -> Option<[u64; 3]> {
    let n = sorted.len();
    let at = |pct: usize| sorted.get((pct * n).div_ceil(100).max(1) - 1).copied();
    Some([at(25)?, at(50)?, at(75)?])
}

/// The per-stage ticks table (issue 56): for each `(commit, start, source)` group and each stage
/// some run reached after its start, k runs that reached it of n counted, and the tick it was first
/// reached at over those k: median, p25 and p75 (`Row::stage_ticks`, the ledger's session ticks).
/// A stage a run held from its first stage tick (a snapshot's own ladder, `in_world` at spawn) is
/// held at the start, not reached: it would read as tick 20 for every run, so those stages are one
/// row, `<first> to <last>, held at the start`, with the runs that held any. Empty when no row
/// kept ticks (batches from before issue 56 with no ledgers), not a table of zeros.
pub fn stage_tick_lines(batches: &[Batch]) -> Vec<String> {
    #[derive(Default)]
    struct Group {
        n: u64,
        reached: BTreeMap<Stage, Vec<u64>>,
        held: BTreeMap<Stage, u64>,
        held_runs: u64,
    }
    let mut groups: BTreeMap<(String, String, Source), Group> = BTreeMap::new();
    for b in batches {
        let g = groups
            .entry((b.commit.clone(), b.start.clone(), b.source))
            .or_default();
        for row in b.rows.iter().filter(|r| r.ending.counted()) {
            g.n += 1;
            let first = row.stage_ticks.iter().map(|t| t.1).min();
            g.held_runs += u64::from(first.is_some());
            for (stage, at) in &row.stage_ticks {
                if Some(*at) == first {
                    *g.held.entry(*stage).or_default() += 1;
                } else {
                    g.reached.entry(*stage).or_default().push(*at);
                }
            }
        }
    }
    if groups
        .values()
        .all(|g| g.reached.is_empty() && g.held.is_empty())
    {
        return Vec::new();
    }
    let mut lines = vec![
        "| commit | start | stage | k/n | median tick | p25 | p75 |".to_owned(),
        "|---|---|---|---|---|---|---|".to_owned(),
    ];
    for ((commit, start, source), g) in &mut groups {
        let short = commit.get(..7).unwrap_or(commit);
        let start = match source {
            Source::Real => start.clone(),
            Source::Sim => format!("{start} (sim)"),
        };
        let held: Vec<&Stage> = g.held.keys().collect();
        if let (Some(lo), Some(hi)) = (held.first(), held.last()) {
            let span = if lo == hi {
                lo.name().to_owned()
            } else {
                format!("{} to {}", lo.name(), hi.name())
            };
            lines.push(format!(
                "| {short} | {start} | {span}, held at the start | {}/{} | - | - | - |",
                g.held_runs, g.n
            ));
        }
        for (stage, at) in g.reached.iter_mut() {
            at.sort_unstable();
            let Some([p25, median, p75]) = quartiles(at) else {
                continue;
            };
            lines.push(format!(
                "| {short} | {start} | {} | {}/{} | {median} | {p25} | {p75} |",
                stage.name(),
                at.len(),
                g.n
            ));
        }
    }
    lines
}

/// The pearls histogram (issue 18b): counted runs by the pearls they gained, fewest first, as
/// `pearls gained: 0 x5, 2 x1, 12 x1 (7 runs)`. Only when a run reached the forest or gained a
/// pearl: no other race has anything to say here.
pub fn pearls_line(batches: &[Batch]) -> Option<String> {
    let rows: Vec<&Row> = batches
        .iter()
        .flat_map(|b| &b.rows)
        .filter(|r| r.ending.counted())
        .collect();
    let any = rows.iter().any(|r| {
        r.pearls.is_some_and(|p| p > 0) || r.stage_reached.is_some_and(|s| s >= Stage::Forest)
    });
    if !any {
        return None;
    }
    let mut by: BTreeMap<i64, u64> = BTreeMap::new();
    let mut unknown = 0u64;
    for r in &rows {
        match r.pearls {
            Some(p) => *by.entry(p).or_default() += 1,
            None => unknown += 1,
        }
    }
    let mut parts: Vec<String> = by.iter().map(|(p, n)| format!("{p} x{n}")).collect();
    if unknown > 0 {
        parts.push(format!("not kept x{unknown}"));
    }
    Some(format!(
        "pearls gained: {} ({} runs)",
        parts.join(", "),
        rows.len()
    ))
}

/// The eyes lines (issue 19b), only when a run had eyes or saw a frame: counted runs by the eyes
/// held at the end, throws per bot with their ends, and frames seen.
/// `eyes held at the end: 0 x2, 12 x1 (3 runs)`,
/// `eye throws per bot: 0 x2, 3 x1; 3 thrown, 2 dropped, 1 shattered, 2 picked up`,
/// `frames seen: 1 of 3 runs, 12 most`.
pub fn eyes_lines(batches: &[Batch]) -> Vec<String> {
    let rows: Vec<&Row> = batches
        .iter()
        .flat_map(|b| &b.rows)
        .filter(|r| r.ending.counted())
        .collect();
    if !rows.iter().any(|r| r.eyes.is_some()) {
        return Vec::new();
    }
    let e = |r: &Row| r.eyes.unwrap_or_default();
    let hist = |f: &dyn Fn(&Row) -> i64| {
        let mut by: BTreeMap<i64, u64> = BTreeMap::new();
        for r in &rows {
            *by.entry(f(r)).or_default() += 1;
        }
        by.iter()
            .map(|(k, n)| format!("{k} x{n}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let sum = |f: fn(story::EyeCounts) -> u32| rows.iter().map(|r| u64::from(f(e(r)))).sum::<u64>();
    let seen = rows.iter().filter(|r| e(r).frames > 0).count();
    let most = rows.iter().map(|r| e(r).frames).max().unwrap_or(0);
    vec![
        format!(
            "eyes held at the end: {} ({} runs)",
            hist(&|r| i64::from(e(r).held)),
            rows.len()
        ),
        format!(
            "eye throws per bot: {}; {} thrown, {} dropped, {} shattered, {} picked up",
            hist(&|r| i64::from(e(r).thrown)),
            sum(|c| c.thrown),
            sum(|c| c.dropped),
            sum(|c| c.shattered),
            sum(|c| c.picked)
        ),
        format!("frames seen: {seen} of {} runs, {most} most", rows.len()),
    ]
}

// >>> E1 (issue 20b)
/// The End lines, only when a run had an End: per bot, crystals left and gone by how, pillars,
/// strikes, the dragon's blows both ways, breath and gaze, the dragon health trail.
/// `end rs-l00-b00: eyes placed 11, entered; crystals 10 seen, left 3, gone struck 7;
/// pillars 7 up (top 38) 6 down; strikes 9; dragon hits 0, blows 0 (0.0 health); breath 0;
/// gaze near 2 (stares 0)` and `dragon health rs-l00-b00: 200@40000 180@41200 …`.
pub fn end_lines(batches: &[Batch]) -> Vec<String> {
    let mut out = Vec::new();
    for r in batches.iter().flat_map(|b| &b.rows) {
        let Some(e) = &r.end else { continue };
        let gone = e
            .crystals_gone
            .iter()
            .map(|(k, n)| format!("{k} {n}"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push(format!(
            "end {}: eyes placed {}{}; crystals {} seen, left {}, gone {}; pillars {} up (top {}) {} down; strikes {}; dragon hits {}, blows {} ({:.1} health); breath {}; gaze near {} (stares {}){}{}",
            r.bot,
            e.eyes_placed,
            if e.entered { ", entered" } else { "" },
            e.crystals_seen,
            e.crystals_left.map_or_else(|| "-".to_owned(), |n| n.to_string()),
            if gone.is_empty() { "none".to_owned() } else { gone },
            e.pillars_up,
            e.pillar_top,
            e.pillars_down,
            e.strikes,
            e.dragon_hits,
            e.dragon_blows,
            e.blow_health,
            e.breath,
            e.near_misses,
            e.stares,
            if e.dragon_dead { "; dragon dead" } else { "" },
            if e.credits { "; credits" } else { "" },
        ));
        if !e.dragon_health.is_empty() {
            out.push(format!(
                "dragon health {}: {}",
                r.bot,
                e.dragon_health
                    .iter()
                    .map(|(t, h)| format!("{h:.0}@{t}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
    }
    out
}

#[cfg(test)]
mod end_tests {
    use super::*;

    /// Issue 20b: a run with an End gets its line and its dragon trail; one without none.
    #[test]
    fn the_end_lines_name_each_bot() {
        let mut e = story::EndCounts {
            eyes_placed: 11,
            entered: true,
            crystals_seen: 10,
            crystals_left: Some(3),
            pillars_up: 7,
            pillar_top: 38,
            pillars_down: 6,
            strikes: 9,
            dragon_health: vec![(100, 200.0), (1300, 180.0)],
            ..story::EndCounts::default()
        };
        e.crystals_gone.insert("struck".to_owned(), 7);
        let row = |bot: &str, end| Row {
            bot: bot.to_owned(),
            stage_reached: Some(Stage::End),
            ending: Ending::Cap,
            signature: Default::default(),
            server_ticks: 0,
            wall_ms: 0,
            case: None,
            recording: None,
            aim: None,
            pearls: None,
            eyes: None,
            end,
            stage_ticks: Vec::new(),
            aim_tick: None,
        };
        let b = Batch {
            format_version: FORMAT_VERSION,
            run: "r".to_owned(),
            commit: "c".to_owned(),
            lane: 0,
            start: "s".to_owned(),
            source: Source::Real,
            config_hash: String::new(),
            bots: 2,
            server: String::new(),
            server_tick_rate: 20.0,
            rows: vec![row("rs-l00-b00", Some(e)), row("rs-l00-b01", None)],
            coverage: Coverage::default(),
        };
        let lines = end_lines(std::slice::from_ref(&b));
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].starts_with("end rs-l00-b00: eyes placed 11, entered; crystals 10 seen, left 3, gone struck 7; pillars 7 up (top 38) 6 down; strikes 9"), "{}", lines[0]);
        assert_eq!(lines[1], "dragon health rs-l00-b00: 200@100 180@1300");
    }
}
// <<< E1

#[cfg(test)]
mod coverage_tests {
    use super::*;

    /// Issue 16e: a column not measured, a wanted packet thrown away and a missing case each
    /// read as blind (`!!`); a skipped packet does not.
    #[test]
    fn blind_spots_are_marked_and_unmeasured_never_reads_as_fine() {
        let mut c = Coverage {
            runs: 2,
            cases: 1,
            recordings: 2,
            ..Coverage::default()
        };
        c.unknown_by_kind.insert("play/sound".to_owned(), 10);
        c.unknown_by_kind
            .insert("play/bundle_delimiter".to_owned(), 4);
        let o = observe::Observe {
            wanted: vec![observe::Entry {
                packet: "play/sound".to_owned(),
                why: "hearing".to_owned(),
            }],
            skipped: vec![observe::Entry {
                packet: "play/bundle_delimiter".to_owned(),
                why: "framing".to_owned(),
            }],
        };
        let text = c.render(Some(&o)).join("\n");
        assert!(text.contains("- cases: 1/2 !!"), "{text}");
        assert!(text.contains("- recordings: 2/2\n"), "{text}");
        assert!(text.contains("- ledgers: not measured !!"), "{text}");
        assert!(
            text.contains("- tasks with a reason: not measured !!"),
            "{text}"
        );
        assert!(
            text.contains("- packets thrown away that the stages need: 10 !!"),
            "{text}"
        );
        assert!(
            text.contains("| play/sound | wanted !! | 10 | 5.0 |"),
            "{text}"
        );
        assert!(
            text.contains("| play/bundle_delimiter | skipped | 4 | 2.0 |"),
            "{text}"
        );
    }

    /// Summing keeps a column unmeasured when any batch did not measure it.
    #[test]
    fn an_unmeasured_batch_keeps_the_sum_unmeasured() {
        let mut a = Coverage {
            ledgers: Some(3),
            ..Coverage::default()
        };
        a.add(&Coverage::default());
        assert_eq!(a.ledgers, None);
        let mut b = Coverage {
            ledgers: Some(3),
            ..Coverage::default()
        };
        b.add(&Coverage {
            ledgers: Some(2),
            ..Coverage::default()
        });
        assert_eq!(b.ledgers, Some(5));
    }
}

#[cfg(test)]
mod text_tests {
    use super::*;

    fn row(at: u64, pos: [f64; 3]) -> ::observe::Senses {
        ::observe::Senses {
            at,
            pos: Some(pos),
            dimension: Some("minecraft:the_nether".to_owned()),
            health: Some(20.0),
            food: Some(16),
            saturation: None,
            air: None,
            on_fire: false,
            held: Some("stone_pickaxe".to_owned()),
            held_uses: Some(57),
            tools: Vec::new(),
            free_slots: 3,
            task: "stairs".to_owned(),
            why: "nether bricks seen, no walk to them".to_owned(),
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
            day_time: None,
            biome: None,
            enderman_ticking: None,
            enderman_frozen: None,
            pickaxe_uses: None,
            filler: None,
            pearls: None,
            kills: None,
            angry_ticking: None,
            angry_frozen: None,
            sword_uses: None,
            eyes: None,
            frames: None,
            dragon_health: None,
            dragon_phase: None,
            crystals: None,
            fountain: None,
            over: None,
            endermen_16: None,
            crystals_left: None,
        }
    }

    /// Wall 1: the cell a row prints is the cell the planner works in, for every sign and
    /// fraction (a rounded -18.3 read -18; the planner's feet cell is -19).
    #[test]
    fn the_printed_cell_is_the_planners_feet_cell() {
        for pos in [
            [30.499, 31.0, -18.3],
            [-0.3, 63.875, 0.7],
            [29.7, 36.42181483855212, -20.65643210844759],
            [0.0, -64.0, -1.0],
            [117.5, 70.5, -117.5],
        ] {
            let (x, y, z) = plan::path::feet_cell(pos);
            assert_eq!(::observe::feet_cell(pos), [x, y, z], "{pos:?}");
        }
    }

    /// The senses row's filler counts what the planner bridges with (issue 17b).
    #[test]
    fn the_senses_filler_is_the_planners_bridge_blocks() {
        assert_eq!(::observe::FILLER, plan::path::BRIDGE_BLOCKS);
    }

    /// Wall 1 as the senses text has it: an old row (no cell kept) prints the position to one
    /// decimal and the floored cell; a new row adds what the feet stand on and the lava; the
    /// dimension is a line of its own when it changes.
    #[test]
    fn a_senses_line_gives_the_cell_and_what_the_feet_stand_on() {
        let old = row(31_300, [30.499, 31.0, -18.3]);
        let mut new = row(31_500, [30.499, 31.88, -18.3]);
        new.cell = Some([30, 31, -19]);
        new.feet_block = Some("soul_sand".to_owned());
        new.under = Some("netherrack".to_owned());
        new.lava = ::observe::Touch::Beside;
        new.cursor = Some(("bucket".to_owned(), 1));
        let text = SensesFile {
            format_version: FORMAT_VERSION,
            run: "r".to_owned(),
            bot: "b".to_owned(),
            rows: vec![old, new],
        }
        .render(1);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[1], "   31300 in minecraft:the_nether");
        assert_eq!(
            lines[2],
            "   31300 30.5,31.0,-18.3 cell 30,31,-19 hp 20.0 food 16 held stone_pickaxe (57 uses) | stairs (nether bricks seen, no walk to them) | "
        );
        assert_eq!(
            lines[3],
            "   31500 30.5,31.9,-18.3 cell 30,31,-19 in soul_sand over netherrack, lava beside hp 20.0 food 16 held stone_pickaxe (57 uses) cursor bucket 1 | stairs (nether bricks seen, no walk to them) | "
        );
        assert_eq!(lines.len(), 4);
    }

    /// Wall 5: the case's head says where the bot stood before anything else, and its position
    /// line gives the feet's cell beside the position.
    #[test]
    fn a_case_opens_with_where_the_bot_stood() {
        let case = Case {
            format_version: FORMAT_VERSION,
            run: "45204423376".to_owned(),
            bot: "rs-l03-b00".to_owned(),
            commit: "c".to_owned(),
            lane: 3,
            start: "snapshot".to_owned(),
            source: Source::Real,
            stage_reached: Some(Stage::Nether),
            ending: Ending::NoProgress {
                for_ticks: 24_000,
                task: "idle".to_owned(),
            },
            death_text: None,
            first_hit: None,
            hits: Vec::new(),
            doing: Doing {
                task: "idle".to_owned(),
                why: String::new(),
            },
            items_gained: BTreeMap::new(),
            items_lost: BTreeMap::new(),
            swings_sent: 0,
            swings_landed: 0,
            health: Some(20.0),
            pos: Some([30.49940297953616, 31.0, -18.3]),
            dimension: Some("minecraft:the_nether".to_owned()),
            server_ticks: 55_940,
            wall_ms: 0,
            recording: None,
            replay: String::new(),
            stream: None,
            plan: None,
            story: Some(story::Story {
                standing: vec![
                    "in minecraft:the_nether at 30.50,31.00,-18.30: cell 30,31,-19 on soul_sand, lava beside".to_owned(),
                ],
                ..story::Story::default()
            }),
        };
        let text = case.render();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[4], "where:");
        assert_eq!(
            lines[5],
            "  in minecraft:the_nether at 30.50,31.00,-18.30: cell 30,31,-19 on soul_sand, lava beside"
        );
        assert!(
            text.contains(
                "health 20.0  at 30.50,31.00,-18.30 (feet cell 30,31,-19) in minecraft:the_nether"
            ),
            "{text}"
        );
    }
}

#[cfg(test)]
mod pearls_tests {
    use super::*;

    fn row(stage: Stage, pearls: Option<i64>, ending: Ending) -> Row {
        Row {
            bot: "b".to_owned(),
            stage_reached: Some(stage),
            ending,
            signature: Default::default(),
            server_ticks: 0,
            wall_ms: 0,
            case: None,
            recording: None,
            aim: None,
            pearls,
            eyes: None,
            end: None,
            stage_ticks: Vec::new(),
            aim_tick: None,
        }
    }

    fn batch(rows: Vec<Row>) -> Batch {
        Batch {
            format_version: FORMAT_VERSION,
            run: "r".to_owned(),
            commit: "c".to_owned(),
            lane: 0,
            start: "s".to_owned(),
            source: Source::Real,
            config_hash: String::new(),
            bots: 1,
            server: String::new(),
            server_tick_rate: 20.0,
            rows,
            coverage: Coverage::default(),
        }
    }

    /// Issue 18b: the histogram counts counted runs by pearls gained, says which rows did not
    /// keep it, and is left out of a race with nothing to say.
    #[test]
    fn the_pearls_histogram_counts_runs_by_pearls_gained() {
        let b = batch(vec![
            row(Stage::Forest, Some(0), Ending::Cap),
            row(Stage::Forest, Some(0), Ending::Cap),
            row(Stage::Pearls6, Some(7), Ending::Cap),
            row(Stage::Pearls12, Some(12), Ending::Cap),
            row(Stage::Forest, None, Ending::Cap),
            row(Stage::Forest, Some(3), Ending::Cut),
        ]);
        assert_eq!(
            pearls_line(std::slice::from_ref(&b)).as_deref(),
            Some("pearls gained: 0 x2, 7 x1, 12 x1, not kept x1 (5 runs)")
        );
        let quiet = batch(vec![row(Stage::Endermen, Some(0), Ending::Cap)]);
        assert_eq!(pearls_line(&[quiet]), None);
        let text = render_summary(&summarize(std::slice::from_ref(&b)), &[b], None);
        assert!(text.contains("pearls gained: 0 x2"), "{text}");
    }

    /// Issue 19b: the eyes lines count counted runs by eyes held and by throws, sum the throws'
    /// ends, and count the runs that saw frames; a race with no eyes has none.
    #[test]
    fn the_eyes_lines_count_held_throws_and_frames() {
        let eyes = |held, thrown, dropped, shattered, picked, frames| {
            Some(story::EyeCounts {
                held,
                thrown,
                dropped,
                shattered,
                picked,
                frames,
            })
        };
        let with = |stage, e| Row {
            eyes: e,
            ..row(stage, Some(0), Ending::Cap)
        };
        let b = batch(vec![
            with(Stage::Overworld, eyes(12, 0, 0, 0, 0, 0)),
            with(Stage::Stronghold, eyes(10, 3, 2, 1, 2, 12)),
            with(Stage::Pearls12, None),
            Row {
                ending: Ending::Cut,
                ..with(Stage::Overworld, eyes(5, 9, 0, 9, 0, 0))
            },
        ]);
        assert_eq!(
            eyes_lines(std::slice::from_ref(&b)),
            vec![
                "eyes held at the end: 0 x1, 10 x1, 12 x1 (3 runs)".to_owned(),
                "eye throws per bot: 0 x2, 3 x1; 3 thrown, 2 dropped, 1 shattered, 2 picked up"
                    .to_owned(),
                "frames seen: 1 of 3 runs, 12 most".to_owned(),
            ]
        );
        assert!(eyes_lines(&[batch(vec![row(Stage::Forest, Some(1), Ending::Cap)])]).is_empty());
        let text = render_summary(&summarize(std::slice::from_ref(&b)), &[b], None);
        assert!(text.contains("frames seen: 1 of 3 runs"), "{text}");
    }
}

#[cfg(test)]
mod stage_tick_tests {
    use super::*;

    fn row(bot: &str, ticks: &[(Stage, u64)], ending: Ending) -> Row {
        Row {
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

    fn batch(rows: Vec<Row>) -> Batch {
        Batch {
            format_version: FORMAT_VERSION,
            run: "r".to_owned(),
            commit: "abcdef0123".to_owned(),
            lane: 0,
            start: "ready".to_owned(),
            source: Source::Real,
            config_hash: "0".to_owned(),
            bots: u32::try_from(rows.len()).unwrap_or(0),
            server: "s".to_owned(),
            server_tick_rate: 20.0,
            coverage: Coverage::default(),
            rows,
        }
    }

    #[test]
    fn quartiles_are_nearest_rank() {
        assert_eq!(quartiles(&[]), None);
        assert_eq!(quartiles(&[7]), Some([7, 7, 7]));
        assert_eq!(quartiles(&[1, 2, 3, 4]), Some([1, 2, 3]));
        assert_eq!(quartiles(&[1, 2, 3, 4, 5]), Some([2, 3, 4]));
        assert_eq!(
            quartiles(&[10, 20, 30, 40, 50, 60, 70, 80]),
            Some([20, 40, 60])
        );
    }

    /// The table: k is the runs that reached a stage of the n counted, the ticks are over those k
    /// only, a cut run is in neither, a stage no run reached has no row, and the stages held from
    /// the start are one row with no ticks.
    #[test]
    fn the_table_gives_each_stage_its_median_and_quartiles() {
        let b = batch(vec![
            row(
                "b0",
                &[(Stage::Ready, 10), (Stage::IronKit, 4000)],
                Ending::Cap,
            ),
            row(
                "b1",
                &[(Stage::Ready, 12), (Stage::IronKit, 2000)],
                Ending::Cap,
            ),
            row(
                "b2",
                &[(Stage::Ready, 11), (Stage::IronKit, 3000)],
                Ending::Death {
                    killer: "creeper".to_owned(),
                },
            ),
            row("b3", &[(Stage::Ready, 9)], Ending::Cap),
            row("b4", &[(Stage::Ready, 1), (Stage::IronKit, 1)], Ending::Cut),
        ]);
        assert_eq!(
            stage_tick_lines(std::slice::from_ref(&b)),
            vec![
                "| commit | start | stage | k/n | median tick | p25 | p75 |".to_owned(),
                "|---|---|---|---|---|---|---|".to_owned(),
                "| abcdef0 | ready | ready, held at the start | 4/4 | - | - | - |".to_owned(),
                "| abcdef0 | ready | iron_kit | 3/4 | 3000 | 2000 | 4000 |".to_owned(),
            ]
        );
        let text = render_summary(&summarize(std::slice::from_ref(&b)), &[b], None);
        assert!(
            text.contains("| abcdef0 | ready | iron_kit | 3/4 | 3000 |"),
            "{text}"
        );
    }

    /// Batches from before issue 56 kept no ticks: no table, not a table of zeros.
    #[test]
    fn rows_without_ticks_print_no_table() {
        let b = batch(vec![row("b0", &[], Ending::Cap)]);
        assert!(stage_tick_lines(&[b]).is_empty());
    }

    /// The aim's line carries its met tick over the runs that met it.
    #[test]
    fn the_aim_line_gives_the_met_tick() {
        let with = |bot: &str, met: bool, at: Option<u64>| Row {
            aim: Some(("iron-kit".to_owned(), met)),
            aim_tick: at,
            ..row(bot, &[], Ending::Cap)
        };
        let b = batch(vec![
            with("b0", true, Some(500)),
            with("b1", true, Some(700)),
            with("b2", false, None),
        ]);
        let text = render_summary(&summarize(std::slice::from_ref(&b)), &[b], None);
        assert!(
            text.contains(
                "aim iron-kit: met 2/3 (66.7%); met at median tick 500 (p25 500, p75 700, 2 runs)"
            ),
            "{text}"
        );
    }

    /// A row written before issue 56, without its fields, still reads: the ticks empty, no aim
    /// tick. The old row is the struct as it stood at f2408e5, encoded as the race wrote it.
    #[test]
    fn an_old_row_reads_with_no_ticks() {
        #[derive(Serialize)]
        struct OldRow {
            bot: String,
            stage_reached: Option<Stage>,
            ending: Ending,
            signature: [String; 4],
            server_ticks: u64,
            wall_ms: u64,
            case: Option<String>,
            recording: Option<String>,
            aim: Option<(String, bool)>,
            pearls: Option<i64>,
        }
        let old = OldRow {
            bot: "b".to_owned(),
            stage_reached: Some(Stage::Nether),
            ending: Ending::Cap,
            signature: Default::default(),
            server_ticks: 9,
            wall_ms: 1,
            case: None,
            recording: None,
            aim: Some(("race".to_owned(), true)),
            pearls: None,
        };
        let bytes = rmp_serde::to_vec_named(&old).unwrap_or_default();
        let row: Row = rmp_serde::from_slice(&bytes).unwrap_or_else(|e| panic!("{e}"));
        assert!(row.stage_ticks.is_empty());
        assert_eq!(row.aim_tick, None);
        assert_eq!(row.stage_reached, Some(Stage::Nether));
        // And a new row round-trips its ticks.
        let new = Row {
            aim_tick: Some(77),
            ..row_with_ticks()
        };
        let bytes = rmp_serde::to_vec_named(&new).unwrap_or_default();
        let back: Row = rmp_serde::from_slice(&bytes).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(back, new);
    }

    fn row_with_ticks() -> Row {
        row("b", &[(Stage::InWorld, 3), (Stage::Log, 400)], Ending::Cap)
    }
}
