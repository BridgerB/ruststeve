//! One bot against one server, as a library: the `bot` binary runs one, the `race` binary runs a
//! lane of them (brain/ruststeve/design/race-and-results.md). The loop joins (handshake, offline
//! login, configuration, play), folds everything the server sends into `World`, steps a tick
//! every 50 ms, records every run when asked, and ends with an `Ending`.

pub mod captured;
pub mod devstart;
pub mod evidence;
pub mod landings;
pub mod live;
pub mod lockstep;
pub mod model;
pub mod progress;
pub mod script;

pub use script::Script;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use net::Inbound;
use protocol::session::opening;
use protocol::{Out, S2c, State};
use record::{CHECKPOINT_TICKS, EffectRecord, EventRecord, Header, Record, Recorder};
use results::{Doing, Ending};
use world::{Event, Inventory, World};

/// How often the bot prints a status line.
const STATUS_EVERY: Duration = Duration::from_secs(10);
/// The loop's tick: the world gets the clock and a fresh seed this often (a game tick).
const TICK_EVERY: Duration = Duration::from_millis(50);
/// Behind the tick schedule by more than this (twenty ticks), the schedule starts again.
const MAX_TICK_LAG: Duration = Duration::from_secs(1);
/// Physics ticks a start settles before the inventory is taken as the start's and a script
/// begins: the kit's slot packets arrive within a tick or two, and a body placed by a teleport
/// comes to rest. `bot::lockstep` settles the same.
pub const SETTLE_TICKS: u64 = 20;

/// What applies a start to the bot once it is in the world (the race's RCON commands); run on
/// its own thread so the loop never blocks on it.
pub type ApplyStart = Box<dyn FnOnce() -> Result<(), String> + Send>;

/// How to run one bot.
pub struct Opts {
    pub host: String,
    pub port: u16,
    pub name: String,
    /// Record the run to this file.
    pub record: Option<PathBuf>,
    /// What to press once the start has settled; `None` stands still (no planner yet).
    pub script: Option<Script>,
    /// End the run (`Cap`) after the script has run this many ticks.
    pub script_ticks: Option<u64>,
    /// What the planner should do; it runs once the start has settled, and ends the run (`Cap`)
    /// when its aim is met or it stops stuck.
    pub aim: Option<plan::Aim>,
    /// The run ends `Cap` after this many server ticks in play.
    pub cap_ticks: Option<u64>,
    /// The job's end: the run ends `Cut` at this instant (never a cap on behaviour).
    pub deadline: Option<Instant>,
    pub start: Option<ApplyStart>,
    /// Print status lines every 10 s.
    pub verbose: bool,
    /// Stream snapshots to steve's local relay (`STEVE_RELAY`, schema/snapshot-v1.md); `None`
    /// streams nothing, and the bot behaves the same either way.
    pub relay: Option<String>,
    /// The race run id (`local-<ms>` for one bot), lane and start: the stream's and the
    /// results' shared names.
    pub run_id: String,
    pub lane: u32,
    pub start_name: String,
    /// Where this run's case file will be, for the stream's `end` frame ("" for none).
    pub case_path: String,
    /// Play against the server model instead of the server at host:port (issue 08).
    pub model: Option<Box<sim::Sim>>,
    /// Keep living after the aim is met, to the cap: the planner's always-rows (escapes,
    /// defence, food) run on (issue 12e's hour).
    pub hold_after_aim: bool,
    /// Respawn after a death and play on, to the cap (the marathon; its server keeps inventory):
    /// the run's deaths are counted in the plan's notes instead of ending it.
    pub revive: bool,
    /// What the bot remembers from a snapshot (`one-bot-first`): seeded into the world at the
    /// start, so a resumed bot knows the portal, the bricks and the rest the snapshot bot saw.
    pub memory: Option<memory::Memory>,
    /// Told each time the run reaches a stage higher than any it held, with the world then (the
    /// race takes a snapshot of the first bot to reach each stage).
    pub on_stage: Option<std::sync::mpsc::Sender<StageUp>>,
    /// End the run `NoProgress` after this many server ticks with no stage, no item kind new to
    /// the run, no more of a goal item than it ever held and no new place (`progress::Progress`,
    /// issue 16e); `None` never does.
    pub no_progress_ticks: Option<u64>,
    /// A file rewritten every `PROGRESS_EVERY_TICKS` with where the run stands, for a look while
    /// the race runs (issue 16e); the run's ledger, senses and story so far are written beside it
    /// as `<name>.ledger.txt`, `<name>.senses.txt` and `<name>.case.txt` (`evidence`).
    pub progress: Option<PathBuf>,
    /// Ask for a snapshot (through `on_stage`) once the run has played this many server ticks:
    /// a place to resume from that is not a stage (`race --snapshot-at`).
    pub snapshot_at: Option<u64>,
}

/// A run reaching a new stage, for the race's snapshot.
pub struct StageUp {
    pub bot: String,
    /// The stage reached (`Stage`), or the highest held when the snapshot is asked for another
    /// reason.
    pub stage: world::stage::Stage,
    pub world: Box<World>,
    pub kind: SnapKind,
}

/// Why a snapshot is asked for. Only `Stage` is a stage snapshot; the others are places to
/// resume a look from and are named and marked apart (`devstart::mark_kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapKind {
    /// The run reached a stage higher than any it held.
    Stage,
    /// The run played the ticks `Opts::snapshot_at` names.
    At(u64),
    /// The run is ending `NoProgress`: the stuck spot, to resume and study.
    Stuck,
}

/// What the bot counts while it runs.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub per_state: BTreeMap<State, u64>,
    pub unknown: BTreeMap<(State, u32), u64>,
    pub decode_errors: u64,
    pub events: u64,
    /// Wall ms from connect, and in play.
    pub wall_ms: u64,
    pub play_ms: u64,
    /// Server ticks when play began and at the end.
    pub play_ticks_from: u64,
}

/// Ticks between senses rows: one a second (clojurecraft's senses line).
pub const SENSES_EVERY_TICKS: u64 = 20;
/// Ticks between progress files, and the run's evidence beside them (`evidence`): two minutes.
/// Ten until issue 16, with the evidence written only at the run's end: a race of 12,000 ticks
/// showed nothing until it was over (docs/problems/race-tooling.md).
pub const PROGRESS_EVERY_TICKS: u64 = 2400;
pub use progress::REGION_SHIFT;

/// Ticks between moments: one a minute.
pub const MOMENT_EVERY_TICKS: u64 = 1200;
/// Moments kept: the last five minutes, then the end.
pub const MOMENTS_KEPT: usize = 5;

/// A moment: the planner whole, with the senses row of that second (the world itself is in the
/// recording's checkpoint of the same minute).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Moment {
    pub at: u64,
    pub senses: observe::Senses,
    pub plan: Option<plan::Plan>,
}

/// How a run went.
pub struct Outcome {
    pub world: World,
    pub ending: Ending,
    pub stats: Stats,
    /// The inventory once the start's kit settled (or once in the world, with no start).
    pub start_inventory: Option<Inventory>,
    pub doing: Doing,
    /// The recording was written to the end.
    pub recorded: bool,
    /// What the live stream sent, dropped and failed, when it ran.
    pub stream: Option<stream::Counts>,
    /// The bot's teleports when its script began: later ones are the server's corrections.
    pub teleports_at_script: Option<u64>,
    /// The lowest health the server told of while the script ran.
    pub min_health_in_script: Option<f32>,
    /// The planner, when the run had an aim, and its time per tick in microseconds.
    pub plan: Option<plan::Plan>,
    pub plan_tick_us: Vec<u64>,
    /// The planner's notes for the case file.
    pub plan_notes: Notes,
    /// Every task the planner started and ended, with its reason and outcome (issue 16e).
    pub ledger: Vec<plan::TaskRecord>,
    /// World facts in order, a senses row a second, and the last moments (issue 16e).
    pub facts: Vec<observe::Stamped>,
    pub senses: Vec<observe::Senses>,
    pub moments: Vec<Moment>,
}

/// Notes kept from a run's start, and from its end: a case file read in one screen. A race of
/// 24,000 ticks made a few hundred notes a bot once alternating pairs were collapsed.
pub const NOTES_HEAD: usize = 80;
pub const NOTES_TAIL: usize = 160;

/// Notes of one kind always kept before a run of that kind is collapsed: a cast's few sites and
/// failures are never left out, a water search's hundreds of walks are.
pub const KIND_KEPT: u32 = 8;

/// The planner's notes for a case file: every note in order, but one whose `what` repeats either
/// of the two kept before it is left out (a hunt's "cut 6 stairs" and "tunnel" alternate for an
/// hour), and so is one whose kind (its `what` without the words holding a number or a bracket)
/// is among the last four kept once `KIND_KEPT` of that kind are kept (a water search's walks
/// differ only in their places: 526 of them filled a case file); past `NOTES_HEAD` +
/// `NOTES_TAIL` the middle is dropped. Left-out notes are counted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notes {
    head: Vec<String>,
    tail: std::collections::VecDeque<String>,
    /// The `what` of the last two notes kept.
    last: [Option<String>; 2],
    /// The kinds of the last four notes kept, and how many of each kind are kept.
    kinds: std::collections::VecDeque<String>,
    kept: BTreeMap<String, u32>,
    pub dropped: u64,
}

/// A note's kind: its `what` without the words that hold a number or a bracket.
fn note_kind(what: &str) -> String {
    what.split_whitespace()
        .filter(|w| {
            !w.chars()
                .any(|c| c.is_ascii_digit() || c == '(' || c == '[')
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A note's kind for a reader: each run of words `note_kind` leaves out shown as one `…`.
pub fn kind_shown(what: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for w in what.split_whitespace() {
        let cut = w
            .chars()
            .any(|c| c.is_ascii_digit() || c == '(' || c == '[');
        if !cut {
            out.push(w);
        } else if out.last() != Some(&"…") {
            out.push("…");
        }
    }
    out.join(" ")
}

impl Notes {
    pub fn push(&mut self, tick: u64, what: &str, why: &str) {
        if self.last.iter().any(|w| w.as_deref() == Some(what)) {
            self.dropped += 1;
            return;
        }
        let kind = note_kind(what);
        if self.kinds.contains(&kind) && self.kept.get(&kind).copied().unwrap_or(0) >= KIND_KEPT {
            self.dropped += 1;
            return;
        }
        *self.kept.entry(kind.clone()).or_default() += 1;
        if self.kinds.len() == 4 {
            self.kinds.pop_front();
        }
        self.kinds.push_back(kind);
        self.last = [self.last[1].take(), Some(what.to_owned())];
        let line = format!("{tick} {what} ({why})");
        if self.head.len() < NOTES_HEAD {
            self.head.push(line);
            return;
        }
        if self.tail.len() == NOTES_TAIL {
            self.tail.pop_front();
            self.dropped += 1;
        }
        self.tail.push_back(line);
    }

    /// The notes kept, oldest first.
    pub fn lines(&self) -> Vec<String> {
        self.head.iter().chain(self.tail.iter()).cloned().collect()
    }
}

impl Outcome {
    /// Items by name gained and lost between the start and the end.
    pub fn items_changed(&self) -> (BTreeMap<String, i64>, BTreeMap<String, i64>) {
        let tally = |inv: &Inventory| {
            let mut m: BTreeMap<String, i64> = BTreeMap::new();
            for (item, n) in inv.slots.values() {
                let name = item
                    .name()
                    .map_or_else(|| format!("unknown:{}", item.id()), str::to_owned);
                *m.entry(name).or_default() += i64::from(*n);
            }
            m
        };
        let before = self.start_inventory.as_ref().map(tally).unwrap_or_default();
        let after = tally(&self.world.inventory);
        let (mut gained, mut lost) = (BTreeMap::new(), BTreeMap::new());
        for name in before.keys().chain(after.keys()) {
            let d = after.get(name).copied().unwrap_or(0) - before.get(name).copied().unwrap_or(0);
            if d > 0 {
                gained.insert(name.clone(), d);
            } else if d < 0 {
                lost.insert(name.clone(), -d);
            }
        }
        (gained, lost)
    }
}

/// A text component's printable characters: enough to read a kick reason in a log.
pub fn printable(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}

/// The shell's randomness: xorshift64 from a clock-derived start. The core never draws its
/// own; it gets these on the tick, and the recording keeps them.
struct Seeds(u64);

impl Seeds {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// The goal table (`config/goals.toml`, issue 10) with its generated producer rows.
pub fn goals_table(path: &str) -> Result<plan::make::Table, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let goals: plan::make::Goals = toml::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    plan::make::Table::new(&goals)
}

/// Packets handled between two ticks at most: the queue is taken whole in all but a flood.
pub const MAX_PACKETS_A_TURN: usize = 4096;

/// `first` and every message already queued behind it, up to `max` in all, and whether the
/// sender is gone. The loop takes the whole queue between two ticks, so a slow tick never leaves
/// a keep-alive waiting behind a backlog (race 45350933958 lane 1).
pub fn batch<T>(rx: &Receiver<T>, first: T, max: usize) -> (Vec<T>, bool) {
    let mut out = vec![first];
    while out.len() < max {
        match rx.try_recv() {
            Ok(m) => out.push(m),
            Err(TryRecvError::Empty) => return (out, false),
            Err(TryRecvError::Disconnected) => return (out, true),
        }
    }
    (out, false)
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// The world and, when recording, its file: every event is written before the step, its
/// effects after, and a checkpoint every `CHECKPOINT_TICKS` server ticks.
struct Run {
    world: World,
    recorder: Option<Recorder>,
    events: u64,
    next_checkpoint: u64,
}

impl Run {
    fn step(
        &mut self,
        conn: &net::Connection,
        record: EventRecord,
        event: Event,
    ) -> Result<(), String> {
        if let Some(r) = &mut self.recorder {
            r.write(&Record::Event(record))?;
        }
        let effects = self.world.step(&event);
        self.events += 1;
        if let Some(r) = &mut self.recorder {
            let recorded = effects
                .iter()
                .map(EffectRecord::of)
                .collect::<Result<Vec<_>, _>>()?;
            r.write(&Record::Effects(recorded))?;
            let ticks = self.world.session.server_ticks;
            if ticks >= self.next_checkpoint {
                r.write(&Record::Checkpoint {
                    event_index: self.events,
                    server_ticks: ticks,
                    world: Box::new(self.world.clone()),
                })?;
                self.next_checkpoint = (ticks / CHECKPOINT_TICKS + 1) * CHECKPOINT_TICKS;
            }
        }
        for e in &effects {
            conn.outbound
                .send(record::as_out(e))
                .map_err(|_| "the writer thread stopped".to_owned())?;
        }
        Ok(())
    }
}

/// The killer in the server's words, without the victim's name, so the same cause groups
/// across bots: `death.attack.mob zombie`; the whole text when it is not a death message.
pub fn killer(d: &protocol::nbt::Death) -> String {
    match (&d.key, &d.by) {
        (Some(k), Some(by)) => format!("{k} {by}"),
        (Some(k), None) => k.clone(),
        (None, _) => d.text.clone(),
    }
}

/// Issue 06's staircase (`Script::WalkSquare`), as a function of the tick.
pub fn walk_controls(tick: u64) -> physics::Controls {
    Script::WalkSquare.controls(tick)
}

/// Where the start stands: not yet in the world, applying, settling, or taken.
enum StartState {
    Waiting(Option<ApplyStart>),
    Applying(JoinHandle<Result<(), String>>),
    /// Counting physics ticks with the ground loaded since the last teleport.
    Settling {
        since: u64,
        teleports: u64,
    },
    Taken,
}

/// Run one bot to its end. `Err` only when it never connected; every run that connected ends
/// with an `Outcome`.
pub fn run(opts: Opts) -> Result<Outcome, String> {
    let Opts {
        host,
        port,
        name,
        record,
        script,
        script_ticks,
        aim,
        cap_ticks,
        deadline,
        start,
        verbose,
        relay,
        run_id,
        lane,
        start_name,
        case_path,
        model,
        hold_after_aim,
        revive,
        memory,
        on_stage,
        no_progress_ticks,
        progress,
        snapshot_at,
    } = opts;
    let (addr, conn) = match model {
        Some(sim) => ("model".to_owned(), model::connect(*sim)),
        None => {
            let addr = format!("{host}:{port}");
            let conn = net::connect(&addr).map_err(|e| format!("connect {addr}: {e}"))?;
            (addr, conn)
        }
    };
    let protocol = i32::try_from(data::VERSION.protocol).map_err(|e| e.to_string())?;
    for p in opening(protocol, &host, port, &name) {
        conn.outbound
            .send(Out::Send(p))
            .map_err(|_| "the writer thread stopped".to_owned())?;
    }
    println!("bot {name}: connecting to {addr} as protocol {protocol}");

    let seed = now_ms() | 1;
    let mut seeds = Seeds(seed);
    // The live stream: an observer on its own thread, fed after each step.
    let sink = relay.as_deref().map(|r| {
        println!("bot {name}: streaming to {r}");
        stream::Sink::start(
            r,
            stream::Identity {
                bot: name.clone(),
                run: run_id.clone(),
                hello: live::hello(seed, lane, &start_name, &addr),
            },
        )
    });
    let mut world_ticks: u64 = 0;
    let recorder = match &record {
        Some(path) => {
            let r = Recorder::create(path, &Header::new(&name, &addr, seed))?;
            println!(
                "bot {name}: recording to {} (commit {})",
                path.display(),
                record::COMMIT
            );
            Some(r)
        }
        None => None,
    };
    let mut run = Run {
        world: World::new(),
        recorder,
        events: 0,
        next_checkpoint: CHECKPOINT_TICKS,
    };
    if let Some(m) = memory {
        run.world.memory = m;
        // A snapshot's memory from before the kind index: built here, or every spawner and
        // fortress look scans the sightings (about 130 ms in a fortress, race 45350933958 lane 1).
        run.world.memory.reindex();
        // The seeded memory is the run's start state and no event carries it: recorded as a
        // checkpoint before the first event, so a fold begins where the run did (`replay verify`
        // counted every checkpoint of every resumed run a mismatch without it).
        if let Some(r) = &mut run.recorder {
            r.write(&Record::Checkpoint {
                event_index: 0,
                server_ticks: 0,
                world: Box::new(run.world.clone()),
            })?;
        }
    }
    let mut stage_told = run.world.stage_reached;
    let mut stats = Stats::default();
    let started = Instant::now();
    let mut play_since: Option<Instant> = None;
    let mut start_state = StartState::Waiting(start);
    let mut start_inventory = None;
    let mut last_status = Instant::now();
    // Ticks on a fixed 20 Hz schedule, as a vanilla client's: each due at the last one's due
    // time plus 50 ms. Waiting for a packet up to a whole tick after each (what this did before
    // issue 13a) let ticks drift 50 to 100 ms apart, the bot playing at about two thirds of a
    // client's pace: found by the parity table's half-cooked furnace rows (a cook of 200 server
    // ticks took 129 bot ticks).
    let mut next_tick = Instant::now() + TICK_EVERY;
    let mut script_tick: u64 = 0;
    let mut teleports_at_script: Option<u64> = None;
    let mut min_health_in_script: Option<f32> = None;
    let mut planner = match aim {
        Some(a) if a.is_goal() => Some(plan::Plan::with_table(
            a,
            std::sync::Arc::new(goals_table("config/goals.toml")?),
        )),
        Some(a) => Some(plan::Plan::new(a)),
        None => None,
    };
    let mut plan_seen: u64 = 0;
    let mut plan_tick_us: Vec<u64> = Vec::new();
    let mut plan_notes = Notes::default();
    // The task ledger, every record of the run, unsampled (issue 16e).
    let mut ledger: Vec<plan::TaskRecord> = Vec::new();
    // What the bot sensed (issue 16e): world facts every tick, a senses row a second, and a
    // moment (the plan, its senses) a minute for the last few minutes.
    let mut facts: Vec<observe::Stamped> = Vec::new();
    let mut senses_rows: Vec<observe::Senses> = Vec::new();
    let mut moments: std::collections::VecDeque<Moment> = std::collections::VecDeque::new();
    let mut last_snap = observe::snapshot(&run.world);
    // Progress (issue 16e): the last stage, new item kind, more of a goal item or new place.
    let mut progressed = progress::Progress::new(if no_progress_ticks.is_some() {
        progress::items_from_repo()
    } else {
        BTreeMap::new()
    });
    let mut last_controls: Option<physics::Controls> = None;
    // The evidence writer of the last progress interval, on its own thread.
    let mut evidence_writer: Option<JoinHandle<()>> = None;
    let mut doing = if let Some(a) = aim {
        Doing {
            task: format!("aim {}", a.name()),
            why: "the planner (issue 09)".to_owned(),
        }
    } else if let Some(s) = script {
        Doing {
            task: format!("script {}", s.name()),
            why: "scripted harness until the planner (issue 09)".to_owned(),
        }
    } else {
        Doing {
            task: "idle".to_owned(),
            why: "no planner yet (issue 09)".to_owned(),
        }
    };

    let mut deaths: u32 = 0;
    let mut snapshot_at_sent = false;
    let ending = 'run: loop {
        if run.world.stage_reached > stage_told {
            stage_told = run.world.stage_reached;
            if let (Some(tx), Some(stage)) = (on_stage.as_ref(), stage_told) {
                let _ = tx.send(StageUp {
                    bot: name.clone(),
                    stage,
                    world: Box::new(run.world.clone()),
                    kind: SnapKind::Stage,
                });
            }
        }
        if let Some(d) = &run.world.death {
            if !revive {
                break Ending::Death { killer: killer(d) };
            }
            // The marathon plays through deaths: respawn, and the planner chooses again from
            // wherever the server puts the body (its items kept by the server's keep_inventory).
            deaths += 1;
            let k = killer(d);
            if let Some(p) = planner.as_mut() {
                p.revived(run.world.session.server_ticks, &k, deaths);
            }
            if let Some(e) = run
                .step(
                    &conn,
                    EventRecord::Act(world::Act::Respawn),
                    Event::Act(world::Act::Respawn),
                )
                .err()
            {
                return Err(e);
            }
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            break Ending::Cut;
        }
        // A snapshot on request: once, when the run has played the ticks asked for.
        if let (Some(at), Some(tx), Some(_)) = (snapshot_at, on_stage.as_ref(), play_since)
            && !snapshot_at_sent
            && run
                .world
                .session
                .server_ticks
                .saturating_sub(stats.play_ticks_from)
                >= at
        {
            snapshot_at_sent = true;
            let _ = tx.send(StageUp {
                bot: name.clone(),
                stage: run.world.stage_reached.unwrap_or(world::Stage::InWorld),
                world: Box::new(run.world.clone()),
                kind: SnapKind::At(at),
            });
        }
        if let (Some(cap), Some(_)) = (cap_ticks, play_since)
            && run
                .world
                .session
                .server_ticks
                .saturating_sub(stats.play_ticks_from)
                >= cap
        {
            break Ending::Cap;
        }
        if verbose && last_status.elapsed() >= STATUS_EVERY {
            last_status = Instant::now();
            println!(
                "bot {name}: {:?} in {}, {} server ticks, health {:?}, stage {:?}",
                run.world.session.state,
                run.world
                    .dimension
                    .as_ref()
                    .map_or("-", |d| d.name.as_str()),
                run.world.session.server_ticks,
                run.world.player.health,
                run.world.stage_reached
            );
        }
        // The start: applied on its own thread once the body is placed, then the inventory
        // is taken as the start's once the kit has settled.
        start_state = match start_state {
            StartState::Waiting(apply) if run.world.body.is_some() => match apply {
                Some(f) => StartState::Applying(std::thread::spawn(f)),
                None => StartState::Settling {
                    since: run.world.ticks,
                    teleports: run.world.session.teleports,
                },
            },
            StartState::Applying(h) if h.is_finished() => match h.join() {
                Ok(Ok(())) => StartState::Settling {
                    since: run.world.ticks,
                    teleports: run.world.session.teleports,
                },
                Ok(Err(e)) => {
                    break Ending::Error {
                        why: format!("start: {e}"),
                    };
                }
                Err(_) => {
                    break Ending::Error {
                        why: "start: the thread panicked".to_owned(),
                    };
                }
            },
            // The start settles over `SETTLE_TICKS` physics ticks with the ground under the body
            // loaded and no teleport since: a teleport far from the loaded columns waits for
            // them, the start's own teleport may land after its command returns, and a script
            // must begin from the same state in a real run as against the model (the parity
            // table found a 20-to-40-tick jitter when this counted server ticks).
            StartState::Settling { teleports, .. }
                if teleports != run.world.session.teleports
                    || !run.world.columns_loaded_under_body() =>
            {
                StartState::Settling {
                    since: run.world.ticks,
                    teleports: run.world.session.teleports,
                }
            }
            StartState::Settling { since, .. } if run.world.ticks - since >= SETTLE_TICKS => {
                start_inventory = Some(run.world.inventory.clone());
                StartState::Taken
            }
            other => other,
        };
        let now = Instant::now();
        if now >= next_tick {
            next_tick += TICK_EVERY;
            // Far behind (the machine stalled): start the schedule again rather than burst.
            if now.saturating_duration_since(next_tick) > MAX_TICK_LAG {
                next_tick = now + TICK_EVERY;
            }
            if let Some(s) = script
                && matches!(start_state, StartState::Taken)
            {
                if script_ticks.is_some_and(|n| script_tick >= n) {
                    break Ending::Cap;
                }
                teleports_at_script.get_or_insert(run.world.session.teleports);
                if let Some(h) = run.world.player.health {
                    min_health_in_script = Some(min_health_in_script.map_or(h, |m: f32| m.min(h)));
                }
                let c = s.steer(script_tick, &run.world);
                let acts = s.acts(script_tick, &run.world);
                script_tick += 1;
                if Some(c) != last_controls {
                    last_controls = Some(c);
                    if let Err(e) = run.step(&conn, EventRecord::Controls(c), Event::Controls(c)) {
                        break Ending::Error { why: e };
                    }
                }
                if let Some(e) = acts
                    .into_iter()
                    .find_map(|a| run.step(&conn, EventRecord::Act(a), Event::Act(a)).err())
                {
                    break Ending::Error { why: e };
                }
            }
            if let Some(p) = planner.as_mut()
                && matches!(start_state, StartState::Taken)
            {
                if p.finished() && !hold_after_aim {
                    break Ending::Cap;
                }
                // A parity row run by the planner (`plan:<aim>`, issue 12b) ends after its ticks
                // and keeps the lowest health, as a scripted one does.
                if script.is_none() {
                    if script_ticks.is_some_and(|n| script_tick >= n) {
                        break Ending::Cap;
                    }
                    script_tick += 1;
                    if let Some(h) = run.world.player.health {
                        min_health_in_script =
                            Some(min_health_in_script.map_or(h, |m: f32| m.min(h)));
                    }
                }
                let t0 = Instant::now();
                let c = p.step(&run.world);
                let acts = p.take_acts();
                plan_tick_us.push(u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX));
                teleports_at_script.get_or_insert(run.world.session.teleports);
                for e in p.events_since(plan_seen) {
                    plan_notes.push(e.tick, &e.what, &e.why);
                    ledger.push(plan::TaskRecord::Note {
                        at: run.world.session.server_ticks,
                        what: e.what.clone(),
                        why: e.why.clone(),
                    });
                    if let Some(r) = &mut run.recorder {
                        let note = format!("plan tick {}: {} ({})", e.tick, e.what, e.why);
                        if let Err(err) = r.write(&Record::Note(note)) {
                            eprintln!("bot {name}: note: {err}");
                        }
                    }
                }
                plan_seen = p.event_count;
                ledger.extend(p.take_records());
                if Some(c) != last_controls {
                    last_controls = Some(c);
                    if let Err(e) = run.step(&conn, EventRecord::Controls(c), Event::Controls(c)) {
                        break Ending::Error { why: e };
                    }
                }
                if let Some(e) = acts
                    .into_iter()
                    .find_map(|a| run.step(&conn, EventRecord::Act(a), Event::Act(a)).err())
                {
                    break Ending::Error { why: e };
                }
            }
            let (now_ms, seed) = (now_ms(), seeds.next());
            if let Err(e) = run.step(
                &conn,
                EventRecord::Tick { now_ms, seed },
                Event::Tick { now_ms, seed },
            ) {
                break Ending::Error { why: e };
            }
            world_ticks += 1;
            let mut snap = observe::snapshot(&run.world);
            let new_facts = observe::diff(&last_snap, &mut snap, &run.world);
            last_snap = snap;
            let now_tick = run.world.session.server_ticks;
            progressed.see(now_tick, &new_facts, run.world.body.map(|b| b.pos));
            facts.extend(new_facts);
            if let Some(n) = no_progress_ticks
                && run.world.body.is_some()
                && progressed.idle(now_tick) >= n
            {
                let task = planner
                    .as_ref()
                    .map_or("none", |p| p.task.name())
                    .to_owned();
                // The stuck spot is kept: a snapshot to resume and study it from, never a stage's.
                if let Some(tx) = on_stage.as_ref() {
                    let _ = tx.send(StageUp {
                        bot: name.clone(),
                        stage: run.world.stage_reached.unwrap_or(world::Stage::InWorld),
                        world: Box::new(run.world.clone()),
                        kind: SnapKind::Stuck,
                    });
                }
                break Ending::NoProgress {
                    for_ticks: progressed.idle(now_tick),
                    task,
                };
            }
            if let Some(path) = &progress
                && world_ticks.is_multiple_of(PROGRESS_EVERY_TICKS)
            {
                let (task, why) = planner
                    .as_ref()
                    .map_or(("none", String::new()), |p| (p.task.name(), p.why()));
                let r = observe::senses(&run.world, task, &why, plan_tick_us.last().copied());
                let line = format!(
                    "{name}\ttick {now_tick}\tstage {}\tat {}\thp {}\tfood {}\theld {}{}\ttask {task} ({why})\tno progress for {} ticks\n",
                    run.world.stage_reached.map_or("none", world::Stage::name),
                    // Where it stands: the cell, what is under it, lava or water at it.
                    r.standing(),
                    r.health
                        .map_or_else(|| "-".to_owned(), |h| format!("{h:.1}")),
                    r.food.map_or_else(|| "-".to_owned(), |f| f.to_string()),
                    r.held.as_deref().unwrap_or("nothing"),
                    // Uses only for what has them: food read "held beef (- uses)".
                    r.held_uses
                        .map_or_else(String::new, |u| format!(" ({u} uses)")),
                    progressed.idle(now_tick),
                );
                if let Err(e) = std::fs::write(path, &line) {
                    eprintln!("bot {name}: progress: {e}");
                }
                // The evidence so far, beside it: copied here, rendered and written on a thread
                // of its own (the loop never waits on the disk). A writer still busy is left be.
                if evidence_writer.as_ref().is_none_or(JoinHandle::is_finished)
                    && let Some(dir) = path.parent()
                {
                    let e = evidence::Evidence {
                        dir: dir.to_path_buf(),
                        run: run_id.clone(),
                        bot: name.clone(),
                        progress: line,
                        tasks: ledger.clone(),
                        facts: facts.clone(),
                        senses: senses_rows.clone(),
                    };
                    let bot = name.clone();
                    evidence_writer = Some(std::thread::spawn(move || {
                        if let Err(err) = e.write() {
                            eprintln!("bot {bot}: evidence: {err}");
                        }
                    }));
                }
            }
            if world_ticks.is_multiple_of(SENSES_EVERY_TICKS) {
                let (task, why) = planner
                    .as_ref()
                    .map_or(("none", String::new()), |p| (p.task.name(), p.why()));
                let row = observe::senses(&run.world, task, &why, plan_tick_us.last().copied());
                if world_ticks.is_multiple_of(MOMENT_EVERY_TICKS) {
                    moments.push_back(Moment {
                        at: run.world.session.server_ticks,
                        senses: row.clone(),
                        plan: planner.clone(),
                    });
                    if moments.len() > MOMENTS_KEPT {
                        moments.pop_front();
                    }
                }
                senses_rows.push(row);
            }
            if let Some(sink) = &sink {
                let tick = run.world.session.server_ticks;
                if world_ticks.is_multiple_of(live::SELF_EVERY_TICKS)
                    && let Some(b) = live::self_state(&run.world, &doing)
                {
                    sink.send(tick, b);
                }
                if world_ticks.is_multiple_of(live::ENTITIES_EVERY_TICKS)
                    && run.world.body.is_some()
                {
                    sink.send(tick, live::entities(&run.world));
                }
            }
        }
        let wait = next_tick
            .saturating_duration_since(Instant::now())
            .max(Duration::from_millis(1));
        let first = match conn.inbound.recv_timeout(wait) {
            Ok(m) => m,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                break Ending::Lost {
                    why: "the reader thread stopped".to_owned(),
                };
            }
        };
        // Every packet already queued is handled before the next tick, not one a tick: with ticks
        // slowed to 130 ms by the planner, one packet a tick left the server's keep-alive queued
        // past its 15 s and the server dropped the bot (race 45350933958 lane 1, "lost
        // connection: Timed out", read by the bot as "the writer thread stopped").
        let (msgs, gone) = batch(&conn.inbound, first, MAX_PACKETS_A_TURN);
        'packets: for msg in msgs {
            let (packet, raw) = match msg {
                Inbound::Packet { packet, raw } => (packet, raw),
                Inbound::DecodeError { state, id, error } => {
                    stats.decode_errors += 1;
                    eprintln!("bot {name}: cannot decode {state:?} id {id}: {error}");
                    continue 'packets;
                }
                Inbound::Closed(why) => break 'run Ending::Lost { why },
            };
            *stats.per_state.entry(run.world.session.state).or_default() += 1;
            match &packet {
                S2c::Unknown { state, id, .. } => {
                    *stats.unknown.entry((*state, *id)).or_default() += 1
                }
                S2c::LoginDisconnect { reason } => {
                    break 'run Ending::Kicked {
                        reason: reason.clone(),
                    };
                }
                S2c::ConfigDisconnect { reason } | S2c::Disconnect { reason } => {
                    break 'run Ending::Kicked {
                        reason: printable(reason),
                    };
                }
                S2c::Login {
                    entity_id,
                    dimension,
                    ..
                } => println!("bot {name}: in play as entity {entity_id} in {dimension}"),
                _ => {}
            }
            let before = run.world.session.state;
            // What the stream will show of this packet, read before the step consumes it.
            let shown = sink
                .as_ref()
                .map(|_| live::from_packet(&packet, Some(&raw.body)))
                .unwrap_or_default();
            let record = EventRecord::Packet {
                state: raw.state,
                id: raw.id,
                body: raw.body,
            };
            if let Err(e) = run.step(&conn, record, Event::Packet(packet)) {
                break 'run Ending::Error { why: e };
            }
            if let Some(sink) = &sink {
                for b in shown {
                    sink.send(run.world.session.server_ticks, b);
                }
            }
            if before != run.world.session.state && run.world.session.state == State::Play {
                play_since.get_or_insert_with(Instant::now);
                stats.play_ticks_from = run.world.session.server_ticks;
            }
        }
        if gone {
            break Ending::Lost {
                why: "the reader thread stopped".to_owned(),
            };
        }
    };
    // The last evidence is whole on disk before the run's end writes the same files.
    if let Some(h) = evidence_writer.take()
        && h.join().is_err()
    {
        eprintln!("bot {name}: the evidence writer panicked");
    }
    stats.events = run.events;
    stats.wall_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    stats.play_ms = play_since.map_or(0, |t| {
        u64::try_from(t.elapsed().as_millis()).unwrap_or(u64::MAX)
    });
    let recorded = match run.recorder.take() {
        Some(r) => match r.finish() {
            Ok(()) => true,
            Err(e) => {
                eprintln!("bot {name}: finishing the recording: {e}");
                false
            }
        },
        None => false,
    };
    println!(
        "bot {name}: ended {ending:?} after {} server ticks, stage {:?}, {} events",
        run.world.session.server_ticks, run.world.stage_reached, stats.events
    );
    // The task still running ends with the run, so every start has an end in the ledger.
    if let Some(p) = planner.as_mut() {
        p.close(run.world.session.server_ticks, &format!("{ending:?}"));
        ledger.extend(p.take_records());
    }
    // The moment at the end, after the last five minutes.
    {
        let (task, why) = planner
            .as_ref()
            .map_or(("none", String::new()), |p| (p.task.name(), p.why()));
        moments.push_back(Moment {
            at: run.world.session.server_ticks,
            senses: observe::senses(&run.world, task, &why, plan_tick_us.last().copied()),
            plan: planner.clone(),
        });
    }
    // The signature names what the planner was doing at the end, as a kind (no cells or counts),
    // not the aim it started with: every run of a race had the same signature (issue 16e). The
    // words left out show as an ellipsis, so the case's `doing:` line does not read as cut off
    // ("stairs down toward the fortress at" was); the numbers themselves are in the story's
    // last lines, and with them every run would be a signature of its own.
    if let Some(e) = planner.as_ref().and_then(|p| p.events.last()) {
        doing = Doing {
            task: kind_shown(&e.what),
            why: kind_shown(&e.why),
        };
    }
    let stream_counts = sink.map(|s| {
        s.send(
            run.world.session.server_ticks,
            live::end(&ending, &run.world, &case_path),
        );
        s.finish()
    });
    Ok(Outcome {
        world: run.world,
        ending,
        stats,
        start_inventory,
        doing,
        recorded,
        stream: stream_counts,
        teleports_at_script,
        min_health_in_script,
        plan: planner,
        plan_tick_us,
        plan_notes,
        ledger,
        facts,
        senses: senses_rows,
        moments: moments.into_iter().collect(),
    })
}

/// The planner's account for a case file: its aim and outcome, its walks and the brain's time
/// per tick (mean, p99, max microseconds).
pub fn plan_summary(p: &plan::Plan, tick_us: &[u64], notes: &Notes) -> results::PlanSummary {
    let mut sorted = tick_us.to_vec();
    sorted.sort_unstable();
    let tick = (!sorted.is_empty()).then(|| {
        let mean = sorted.iter().sum::<u64>() / sorted.len() as u64;
        let p99 = sorted[(sorted.len() * 99 / 100).min(sorted.len() - 1)];
        [mean, p99, *sorted.last().unwrap_or(&0)]
    });
    results::PlanSummary {
        aim: p.aim.name(),
        reached: p.reached.is_some() || p.met,
        stuck: p.stuck,
        walks: p.walks,
        failures: p.failures.clone(),
        longest_stall: p.longest_stall,
        expanded: p.expanded,
        tick_us: tick,
        notes: notes.lines(),
        notes_dropped: notes.dropped,
    }
}

#[cfg(test)]
mod notes_tests {
    use super::{KIND_KEPT, NOTES_HEAD, NOTES_TAIL, Notes, kind_shown};

    #[test]
    fn a_kind_shows_where_words_were_left_out() {
        assert_eq!(
            kind_shown("stairs down toward the fortress at (61, 30, 95)"),
            "stairs down toward the fortress at …"
        );
        assert_eq!(
            kind_shown("stairs failed: DigFailed after 0 stairs"),
            "stairs failed: DigFailed after … stairs"
        );
        assert_eq!(
            kind_shown("nether bricks seen, no walk to them"),
            "nether bricks seen, no walk to them"
        );
    }

    #[test]
    fn alternating_pairs_collapse_and_are_counted() {
        let mut n = Notes::default();
        n.push(1, "cast at (1, 2, 3)", "a site");
        for t in 0..10 {
            n.push(10 + t, "cut 6 stairs", "look again");
            n.push(11 + t, "tunnel", "no lava in sight");
        }
        n.push(99, "cast failed: NoRefill", "site given up");
        assert_eq!(
            n.lines(),
            [
                "1 cast at (1, 2, 3) (a site)",
                "10 cut 6 stairs (look again)",
                "11 tunnel (no lava in sight)",
                "99 cast failed: NoRefill (site given up)",
            ]
        );
        assert_eq!(n.dropped, 18);
    }

    /// A word of letters only, a different one for each number: a note kind of its own.
    fn word(i: usize) -> String {
        let letter = |d: usize| char::from(b'a' + u8::try_from(d % 26).unwrap());
        [letter(i / 676), letter(i / 26), letter(i)]
            .iter()
            .collect()
    }

    #[test]
    fn the_middle_is_dropped_past_the_head_and_tail() {
        let mut n = Notes::default();
        let total = NOTES_HEAD + NOTES_TAIL + 25;
        for i in 0..total {
            n.push(i as u64, &format!("step {}", word(i)), "why");
        }
        let line = |i: usize| format!("{i} step {} (why)", word(i));
        let lines = n.lines();
        assert_eq!(lines.len(), NOTES_HEAD + NOTES_TAIL);
        assert_eq!(lines[0], line(0));
        assert_eq!(lines[NOTES_HEAD - 1], line(NOTES_HEAD - 1));
        assert_eq!(lines[NOTES_HEAD], line(NOTES_HEAD + 25));
        assert_eq!(lines.last().unwrap(), &line(total - 1));
        assert_eq!(n.dropped, 25);
    }

    /// Race 44052157794, rs-l05-b05: a water search's walks, each to another place, filled the
    /// case file (526 notes left out by the other rules, 240 kept).
    #[test]
    fn a_run_of_one_kind_collapses_after_its_first_eight() {
        let mut n = Notes::default();
        for i in 0..30u64 {
            n.push(i, &format!("walk toward water at ({i}, 26, -90)"), "a pool");
            n.push(
                i,
                &format!("explore to ({i}, 26, -90) failed: NoPath"),
                "turn",
            );
        }
        n.push(99, "cast at (1, 2, 3) facing south", "a site");
        let lines = n.lines();
        assert_eq!(lines.len(), 2 * KIND_KEPT as usize + 1);
        assert_eq!(n.dropped, 60 - 2 * u64::from(KIND_KEPT));
        assert_eq!(
            lines.last().unwrap(),
            "99 cast at (1, 2, 3) facing south (a site)"
        );
    }
}

#[cfg(test)]
mod batch_tests {
    use super::batch;
    use std::sync::mpsc::channel;

    /// The whole queue is taken after one wait: a keep-alive behind a backlog is answered in the
    /// same turn, not one packet a tick later (race 45350933958 lane 1).
    #[test]
    fn the_whole_queue_is_taken_in_one_turn() {
        let (tx, rx) = channel();
        for i in 1..=300 {
            tx.send(i).unwrap();
        }
        let first = rx.recv().unwrap();
        let (got, gone) = batch(&rx, first, 4096);
        assert_eq!(got, (1..=300).collect::<Vec<_>>());
        assert!(!gone);
        // A cap bounds a flood; what is left waits for the next turn.
        for i in 1..=10 {
            tx.send(i).unwrap();
        }
        let first = rx.recv().unwrap();
        assert_eq!(batch(&rx, first, 4).0, vec![1, 2, 3, 4]);
        // The sender gone is said once the queue is empty.
        drop(tx);
        let first = rx.recv().unwrap();
        let (got, gone) = batch(&rx, first, 4096);
        assert_eq!(got, vec![5, 6, 7, 8, 9, 10]);
        assert!(gone);
    }
}
