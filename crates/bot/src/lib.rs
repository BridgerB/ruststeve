//! One bot against one server, as a library: the `bot` binary runs one, the `race` binary runs a
//! lane of them (brain/ruststeve/design/race-and-results.md). The loop joins (handshake, offline
//! login, configuration, play), folds everything the server sends into `World`, steps a tick
//! every 50 ms, records every run when asked, and ends with an `Ending`.

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
/// Server ticks after a start is applied before the inventory is taken as the start's: the
/// kit's slot packets arrive within a tick or two.
const KIT_SETTLE_TICKS: u64 = 20;

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
    /// Walk issue 06's scripted staircase instead of standing still.
    pub walk: bool,
    /// The run ends `Cap` after this many server ticks in play.
    pub cap_ticks: Option<u64>,
    /// The job's end: the run ends `Cut` at this instant (never a cap on behaviour).
    pub deadline: Option<Instant>,
    pub start: Option<ApplyStart>,
    /// Print status lines every 10 s.
    pub verbose: bool,
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

/// Issue 06's scripted walk, a test harness until the planner (issue 09): a staircase, 40 ticks
/// south then 40 west, over and over, jumping for a few ticks at the start of every west leg.
pub fn walk_controls(tick: u64) -> physics::Controls {
    let side = (tick / 40) % 4;
    let into_side = tick % 40;
    physics::Controls {
        forward: true,
        jump: side % 2 == 1 && into_side < 6,
        sneak: false,
        yaw: [0.0, 90.0, 0.0, 90.0][side as usize],
    }
}

/// Where the start stands: not yet in the world, applying, settling, or taken.
enum StartState {
    Waiting(Option<ApplyStart>),
    Applying(JoinHandle<Result<(), String>>),
    Settling { until_tick: u64 },
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
        walk,
        cap_ticks,
        deadline,
        start,
        verbose,
    } = opts;
    let addr = format!("{host}:{port}");
    let conn = net::connect(&addr).map_err(|e| format!("connect {addr}: {e}"))?;
    let protocol = i32::try_from(data::VERSION.protocol).map_err(|e| e.to_string())?;
    for p in opening(protocol, &host, port, &name) {
        conn.outbound
            .send(Out::Send(p))
            .map_err(|_| "the writer thread stopped".to_owned())?;
    }
    println!("bot {name}: connecting to {addr} as protocol {protocol}");

    let seed = now_ms() | 1;
    let mut seeds = Seeds(seed);
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
    let mut walk_ticks: u64 = 0;
    let mut last_controls: Option<physics::Controls> = None;
    let doing = if walk {
        Doing {
            task: "walk-square".to_owned(),
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
                    until_tick: run.world.session.server_ticks + KIT_SETTLE_TICKS,
                },
            },
            StartState::Applying(h) if h.is_finished() => match h.join() {
                Ok(Ok(())) => StartState::Settling {
                    until_tick: run.world.session.server_ticks + KIT_SETTLE_TICKS,
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
            StartState::Settling { until_tick } if run.world.session.server_ticks >= until_tick => {
                start_inventory = Some(run.world.inventory.clone());
                StartState::Taken
            }
            other => other,
        };
        if last_tick.elapsed() >= TICK_EVERY {
            last_tick = Instant::now();
            if walk && run.world.body.is_some() {
                let c = walk_controls(walk_ticks);
                walk_ticks += 1;
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
        let record = EventRecord::Packet {
            state: raw.state,
            id: raw.id,
            body: raw.body,
        };
        if let Err(e) = run.step(&conn, record, Event::Packet(packet)) {
            break Ending::Error { why: e };
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
    Ok(Outcome {
        world: run.world,
        ending,
        stats,
        start_inventory,
        doing,
        recorded,
    })
}
