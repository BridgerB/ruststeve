//! One bot against one server, as a library: the `bot` binary runs one, the `race` binary runs a
//! lane of them (brain/ruststeve/design/race-and-results.md). The loop joins (handshake, offline
//! login, configuration, play), folds everything the server sends into `World`, steps a tick
//! every 50 ms, records every run when asked, and ends with an `Ending`.

pub mod captured;
pub mod live;
pub mod lockstep;
pub mod model;
pub mod script;

pub use script::Script;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
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
    let mut stats = Stats::default();
    let started = Instant::now();
    let mut play_since: Option<Instant> = None;
    let mut start_state = StartState::Waiting(start);
    let mut start_inventory = None;
    let mut last_status = Instant::now();
    let mut last_tick = Instant::now();
    let mut script_tick: u64 = 0;
    let mut teleports_at_script: Option<u64> = None;
    let mut min_health_in_script: Option<f32> = None;
    let mut planner = aim.map(plan::Plan::new);
    let mut plan_seen: u64 = 0;
    let mut plan_tick_us: Vec<u64> = Vec::new();
    let mut last_controls: Option<physics::Controls> = None;
    let doing = if let Some(a) = aim {
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

    let ending = loop {
        if let Some(d) = &run.world.death {
            break Ending::Death { killer: killer(d) };
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            break Ending::Cut;
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
        if last_tick.elapsed() >= TICK_EVERY {
            last_tick = Instant::now();
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
                let c = s.controls(script_tick);
                script_tick += 1;
                if Some(c) != last_controls {
                    last_controls = Some(c);
                    if let Err(e) = run.step(&conn, EventRecord::Controls(c), Event::Controls(c)) {
                        break Ending::Error { why: e };
                    }
                }
            }
            if let Some(p) = planner.as_mut()
                && matches!(start_state, StartState::Taken)
            {
                if p.reached.is_some() || p.stuck {
                    break Ending::Cap;
                }
                let t0 = Instant::now();
                let c = p.step(&run.world);
                plan_tick_us.push(u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX));
                teleports_at_script.get_or_insert(run.world.session.teleports);
                for e in p.events_since(plan_seen) {
                    if let Some(r) = &mut run.recorder {
                        let note = format!("plan tick {}: {} ({})", e.tick, e.what, e.why);
                        if let Err(err) = r.write(&Record::Note(note)) {
                            eprintln!("bot {name}: note: {err}");
                        }
                    }
                }
                plan_seen = p.event_count;
                if Some(c) != last_controls {
                    last_controls = Some(c);
                    if let Err(e) = run.step(&conn, EventRecord::Controls(c), Event::Controls(c)) {
                        break Ending::Error { why: e };
                    }
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
        let msg = match conn.inbound.recv_timeout(TICK_EVERY) {
            Ok(m) => m,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                break Ending::Lost {
                    why: "the reader thread stopped".to_owned(),
                };
            }
        };
        let (packet, raw) = match msg {
            Inbound::Packet { packet, raw } => (packet, raw),
            Inbound::DecodeError { state, id, error } => {
                stats.decode_errors += 1;
                eprintln!("bot {name}: cannot decode {state:?} id {id}: {error}");
                continue;
            }
            Inbound::Closed(why) => break Ending::Lost { why },
        };
        *stats.per_state.entry(run.world.session.state).or_default() += 1;
        match &packet {
            S2c::Unknown { state, id, .. } => *stats.unknown.entry((*state, *id)).or_default() += 1,
            S2c::LoginDisconnect { reason } => {
                break Ending::Kicked {
                    reason: reason.clone(),
                };
            }
            S2c::ConfigDisconnect { reason } | S2c::Disconnect { reason } => {
                break Ending::Kicked {
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
            break Ending::Error { why: e };
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
    };
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
    })
}

/// The planner's account for a case file: its aim and outcome, its walks and the brain's time
/// per tick (mean, p99, max microseconds).
pub fn plan_summary(p: &plan::Plan, tick_us: &[u64]) -> results::PlanSummary {
    let mut sorted = tick_us.to_vec();
    sorted.sort_unstable();
    let tick = (!sorted.is_empty()).then(|| {
        let mean = sorted.iter().sum::<u64>() / sorted.len() as u64;
        let p99 = sorted[(sorted.len() * 99 / 100).min(sorted.len() - 1)];
        [mean, p99, *sorted.last().unwrap_or(&0)]
    });
    results::PlanSummary {
        aim: p.aim.name(),
        reached: p.reached.is_some(),
        stuck: p.stuck,
        walks: p.walks,
        failures: p.failures.clone(),
        longest_stall: p.longest_stall,
        expanded: p.expanded,
        tick_us: tick,
    }
}
